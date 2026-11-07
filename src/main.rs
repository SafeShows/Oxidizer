//! The standalone launcher: hosts the game's .NET runtime via hostfxr, injects the C# bridge, hands it to
//! `oxidizer_core` (src/lib.rs), which loads the mods, then runs KSA's Main.
//!
//! Usage: oxidizer [--game <KSA dir>] [-InstancePath <dir>] [game args...]
//!        oxidizer [--game <KSA dir>] --bindgen <file.rs>   (write typed Rust bindings for KSA.dll, don't start the game)
//! Game dir can also be set in `oxidizer.toml` next to the exe: `game_path = '...'`.
//! The instance (the folder used instead of Documents/My Games/Kitten Space Agency)
//! can also come from the OXIDIZER_INSTANCE_PATH environment variable.

use libloading::Library;
use oxidizer_core::{BridgeInit, log};
use std::ffi::c_void;
use std::path::PathBuf;
use std::{env, fs};

#[cfg(windows)]
const DEFAULT_GAME: &str = r"C:\Program Files\Ahwoo\Games\Kitten Space Agency";
#[cfg(not(windows))]
const DEFAULT_GAME: &str = "";

/// hostfxr's `char_t`: UTF-16 on Windows, UTF-8 elsewhere.
#[cfg(windows)]
type CharT = u16;
#[cfg(not(windows))]
type CharT = u8;

type InitCmdLine = unsafe extern "C" fn(i32, *const *const CharT, *const InitParams, *mut *mut c_void) -> i32;
type GetDelegate = unsafe extern "C" fn(*mut c_void, i32, *mut *mut c_void) -> i32;
type RunApp = unsafe extern "C" fn(*mut c_void) -> i32;
type LoadAssembly = unsafe extern "C" fn(*const CharT, *mut c_void, *mut c_void) -> i32;
type GetFnPtr = unsafe extern "C" fn(*const CharT, *const CharT, *const CharT, *mut c_void, *mut c_void, *mut *mut c_void) -> i32;

const HDT_GET_FUNCTION_POINTER: i32 = 6;
const HDT_LOAD_ASSEMBLY: i32 = 7;
const UNMANAGED_CALLERS_ONLY: *const CharT = usize::MAX as *const CharT;

#[repr(C)]
struct InitParams {
    size: usize,
    host_path: *const CharT,
    dotnet_root: *const CharT,
}

/// A NUL-terminated string for hostfxr.
fn host_str(s: impl AsRef<std::ffi::OsStr>) -> Vec<CharT> {
    #[cfg(windows)]
    let units = std::os::windows::ffi::OsStrExt::encode_wide(s.as_ref());
    #[cfg(not(windows))]
    let units = std::os::unix::ffi::OsStrExt::as_bytes(s.as_ref()).iter().copied();
    units.chain([0]).collect()
}

fn check(what: &str, rc: i32) {
    if rc < 0 {
        fail(&format!("{what} failed: 0x{:08x}", rc as u32));
    }
}

fn fail(msg: &str) -> ! {
    log(msg);
    std::process::exit(1)
}

fn game_dir(args: &mut Vec<String>) -> PathBuf {
    if let Some(i) = args.iter().position(|a| a == "--game") {
        let v = args.get(i + 1).cloned().unwrap_or_else(|| fail("--game needs a path"));
        args.drain(i..=i + 1);
        return v.into();
    }
    let cfg = env::current_exe().unwrap().with_file_name("oxidizer.toml");
    match fs::read_to_string(&cfg) {
        Ok(s) => {
            let t: toml::Table = s.parse().unwrap_or_else(|e| fail(&format!("{}: {e}", cfg.display())));
            t.get("game_path").and_then(|v| v.as_str()).unwrap_or(DEFAULT_GAME).into()
        }
        Err(_) => {
            let _ = fs::write(&cfg, format!("game_path = '{DEFAULT_GAME}'\n"));
            DEFAULT_GAME.into()
        }
    }
}

/// `-InstancePath <dir>` (removed from the game's arguments) or OXIDIZER_INSTANCE_PATH, as Borea passes it.
fn instance_path(args: &mut Vec<String>) -> String {
    if let Some(i) = args.iter().position(|a| a.eq_ignore_ascii_case("-InstancePath")) {
        let v = args.get(i + 1).cloned().unwrap_or_else(|| fail("-InstancePath needs a folder"));
        args.drain(i..=i + 1);
        return v;
    }
    env::var("OXIDIZER_INSTANCE_PATH").unwrap_or_default()
}

/// `--bindgen <file.rs>`: the (absolute) output file for typed bindings.
fn bindgen_out(args: &mut Vec<String>) -> Option<PathBuf> {
    let i = args.iter().position(|a| a == "--bindgen")?;
    let v = args.get(i + 1).cloned().unwrap_or_else(|| fail("--bindgen needs an output file, e.g. --bindgen src/ksa.rs"));
    args.drain(i..=i + 1);
    Some(std::path::absolute(v).unwrap())
}

fn bridge_dir() -> PathBuf {
    let exe = env::current_exe().unwrap();
    let dir = exe.parent().unwrap();
    [dir.join("bridge"), dir.join("../bridge")]
        .into_iter()
        .find(|d| d.join("Oxidizer.Bridge.dll").exists())
        .unwrap_or_else(|| fail("bridge/Oxidizer.Bridge.dll not found next to the exe (build bridge/ first)"))
}

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let bindgen = bindgen_out(&mut args);
    let game = game_dir(&mut args);
    let instance = instance_path(&mut args);
    let ksa_dll = game.join("KSA.dll");
    if !ksa_dll.exists() {
        fail(&format!("KSA.dll not found in '{}'; set game_path in oxidizer.toml next to the exe, or pass --game <folder>", game.display()));
    }
    let bridge = bridge_dir();
    env::set_current_dir(&game).unwrap();

    // The game ships its own runtime (self-contained), so use its hostfxr.
    let hostfxr_path = game.join(libloading::library_filename("hostfxr"));
    let hostfxr = unsafe { Library::new(&hostfxr_path) }.unwrap_or_else(|e| fail(&format!("{}: {e}", hostfxr_path.display())));
    unsafe {
        let init: libloading::Symbol<InitCmdLine> = hostfxr.get(b"hostfxr_initialize_for_dotnet_command_line\0").unwrap();
        let get_delegate: libloading::Symbol<GetDelegate> = hostfxr.get(b"hostfxr_get_runtime_delegate\0").unwrap();
        let run_app: libloading::Symbol<RunApp> = hostfxr.get(b"hostfxr_run_app\0").unwrap();

        let argv_s: Vec<Vec<CharT>> = std::iter::once(host_str(&ksa_dll)).chain(args.iter().map(host_str)).collect();
        let argv: Vec<*const CharT> = argv_s.iter().map(|a| a.as_ptr()).collect();
        let (host_path, root) = (host_str(env::current_exe().unwrap()), host_str(&game));
        let params = InitParams { size: size_of::<InitParams>(), host_path: host_path.as_ptr(), dotnet_root: root.as_ptr() };
        let mut ctx = std::ptr::null_mut();
        check("hostfxr init", init(argv.len() as i32, argv.as_ptr(), &params, &mut ctx));

        let mut p = std::ptr::null_mut();
        check("get load_assembly", get_delegate(ctx, HDT_LOAD_ASSEMBLY, &mut p));
        let load_assembly: LoadAssembly = std::mem::transmute(p);
        check("get get_function_pointer", get_delegate(ctx, HDT_GET_FUNCTION_POINTER, &mut p));
        let get_fn: GetFnPtr = std::mem::transmute(p);

        for dll in ["0Harmony.dll", "Oxidizer.Bridge.dll"] {
            let path = host_str(bridge.join(dll));
            check(dll, load_assembly(path.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()));
        }
        if let Some(out) = bindgen {
            let (ty, method) = (host_str("Oxidizer.Bindgen, Oxidizer.Bridge"), host_str("Run"));
            check("get Bindgen.Run", get_fn(ty.as_ptr(), method.as_ptr(), UNMANAGED_CALLERS_ONLY, std::ptr::null_mut(), std::ptr::null_mut(), &mut p));
            let run: unsafe extern "C" fn(*const u8, usize) -> i32 = std::mem::transmute(p);
            let out = out.to_string_lossy();
            std::process::exit(if run(out.as_ptr(), out.len()) == 1 { 0 } else { 1 });
        }
        let (ty, method) = (host_str("Oxidizer.Bridge, Oxidizer.Bridge"), host_str("Init"));
        check("get Bridge.Init", get_fn(ty.as_ptr(), method.as_ptr(), UNMANAGED_CALLERS_ONLY, std::ptr::null_mut(), std::ptr::null_mut(), &mut p));
        let bridge_init: BridgeInit = std::mem::transmute(p);

        if let Err(e) = oxidizer_core::attach(bridge_init, &instance) {
            fail(&e);
        }
        let rc = run_app(ctx);
        oxidizer_core::shutdown();
        std::process::exit(rc);
    }
}
