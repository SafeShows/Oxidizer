//! KSA mod loader: hosts the game's .NET runtime via hostfxr, injects the C# bridge,
//! loads Rust (cdylib) and Rhai mods, then runs KSA's Main.
//!
//! Usage: oxidizer [--game <KSA dir>] [-InstancePath <dir>] [game args...]
//! Game dir can also be set in `oxidizer.toml` next to the exe: `game_path = '...'`.
//! The instance (the folder used instead of Documents/My Games/Kitten Space Agency)
//! can also come from the OXIDIZER_INSTANCE_PATH environment variable.

mod script;
mod ui;

use oxidizer_api::{Api, imgui};
use libloading::Library;
use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
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
type BridgeInit = unsafe extern "C" fn(*mut Api, *const Host) -> i32;

const HDT_GET_FUNCTION_POINTER: i32 = 6;
const HDT_LOAD_ASSEMBLY: i32 = 7;
const UNMANAGED_CALLERS_ONLY: *const CharT = usize::MAX as *const CharT;

#[repr(C)]
struct InitParams {
    size: usize,
    host_path: *const CharT,
    dotnet_root: *const CharT,
}

/// Callbacks and settings for the bridge. Field order must match `Host` in `bridge/Bridge.cs`.
#[repr(C)]
struct Host {
    frame: extern "C" fn(f64, f64),
    gui: extern "C" fn(f64),
    menu: extern "C" fn(),
    /// UTF-8 instance folder, empty for the normal Documents folder.
    instance: *const u8,
    instance_len: usize,
}

struct RustMod {
    frame: Option<extern "C" fn(f64, f64)>,
    gui: Option<extern "C" fn(f64)>,
    menu: Option<extern "C" fn()>,
    unload: Option<extern "C" fn()>,
}

static RUST_MODS: OnceLock<Vec<RustMod>> = OnceLock::new();

pub fn log(s: &str) {
    println!("[oxidizer] {s}");
    ui::push_log(s.to_string());
}

extern "C" fn api_log(p: *const u8, n: usize) {
    log(&String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p, n) }));
}

extern "C" fn on_frame(t: f64, dt: f64) {
    script::reload_changed();
    for m in RUST_MODS.get().into_iter().flatten() {
        m.frame.map(|f| f(t, dt));
    }
    script::call_all("frame", (t, dt));
}

extern "C" fn on_gui(dt: f64) {
    for m in RUST_MODS.get().into_iter().flatten() {
        m.gui.map(|f| f(dt));
    }
    script::call_all("gui", (dt,));
    ui::draw();
}

/// Runs inside the game's menu bar (after its own menus).
extern "C" fn on_menu() {
    if imgui::begin_menu("Mods") {
        ui::menu_item();
        for m in RUST_MODS.get().into_iter().flatten() {
            m.menu.map(|f| f());
        }
        script::call_all("menu", ());
        imgui::end_menu();
    }
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
        let (ty, method) = (host_str("Oxidizer.Bridge, Oxidizer.Bridge"), host_str("Init"));
        check("get Bridge.Init", get_fn(ty.as_ptr(), method.as_ptr(), UNMANAGED_CALLERS_ONLY, std::ptr::null_mut(), std::ptr::null_mut(), &mut p));
        let bridge_init: BridgeInit = std::mem::transmute(p);

        let api: &'static mut MaybeUninit<Api> = Box::leak(Box::new(MaybeUninit::uninit()));
        (&raw mut (*api.as_mut_ptr()).log).write(api_log);
        let host = Host { frame: on_frame, gui: on_gui, menu: on_menu, instance: instance.as_ptr(), instance_len: instance.len() };
        if bridge_init(api.as_mut_ptr(), &host) == 0 {
            fail("bridge init failed");
        }
        let api: &'static Api = api.assume_init_ref();
        oxidizer_api::set_api(api);

        load_mods(api);
        let rc = run_app(ctx);

        for m in RUST_MODS.get().into_iter().flatten() {
            m.unload.map(|f| f());
        }
        script::call_all("unload", ());
        std::process::exit(rc);
    }
}

/// Mod folder = `<Documents>/My Games/Kitten Space Agency/mods/<id>/mod.toml` with
/// `[oxidizer] dll = "x.dll"` and/or `script = "main.rhai"`. Respects `enabled = false` in manifest.toml.
fn load_mods(api: &'static Api) {
    let docs = PathBuf::from(oxidizer_api::read_string(|b, c| (api.docs_path)(b, c)));
    let disabled = disabled_mods(&docs.join("manifest.toml"));
    let mut rust = vec![];
    let mut scripts = vec![];
    let Ok(dirs) = fs::read_dir(docs.join("mods")) else {
        return log(&format!("no mods folder in {}", docs.display()));
    };
    for dir in dirs.flatten().map(|e| e.path()) {
        let id = dir.file_name().unwrap().to_string_lossy().into_owned();
        let Some(cfg) = fs::read_to_string(dir.join("mod.toml")).ok().and_then(|s| s.parse::<toml::Table>().ok()) else { continue };
        let Some(section) = cfg.get("oxidizer").and_then(|v| v.as_table()) else { continue };
        if disabled.contains(&id) {
            log(&format!("{id}: disabled in manifest"));
            ui::set_status(&id, "mod", Some("disabled in the game's mod list".into()));
            continue;
        }
        if let Some(dll) = section.get("dll").and_then(|v| v.as_str()) {
            match unsafe { load_rust_mod(&mod_library(&dir, dll), api) } {
                Ok(m) => {
                    log(&format!("loaded rust mod {id}"));
                    ui::set_status(&id, "Rust", None);
                    rust.push(m);
                }
                Err(e) => {
                    log(&format!("{id}: {e}"));
                    ui::set_status(&id, "Rust", Some(e.to_string()));
                }
            }
        }
        if let Some(s) = section.get("script").and_then(|v| v.as_str()) {
            scripts.push((id.clone(), dir.join(s)));
        }
    }
    let _ = RUST_MODS.set(rust);
    script::load(scripts);
}

/// `dll` as written if that file exists, otherwise this platform's library file for its stem,
/// so `dll = "my_mod"` (or "my_mod.dll") finds `my_mod.dll` on Windows and `libmy_mod.so` on Linux.
fn mod_library(dir: &Path, dll: &str) -> PathBuf {
    let exact = dir.join(dll);
    if exact.exists() {
        return exact;
    }
    let stem = Path::new(dll).file_stem().unwrap_or(dll.as_ref());
    dir.join(libloading::library_filename(stem))
}

fn disabled_mods(manifest: &Path) -> Vec<String> {
    let Some(t) = fs::read_to_string(manifest).ok().and_then(|s| s.parse::<toml::Table>().ok()) else { return vec![] };
    t.get("mods").and_then(|m| m.as_array()).into_iter().flatten()
        .filter(|m| m.get("enabled").and_then(|e| e.as_bool()) == Some(false))
        .filter_map(|m| m.get("id")?.as_str().map(String::from))
        .collect()
}

unsafe fn load_rust_mod(path: &Path, api: &'static Api) -> Result<RustMod, Box<dyn std::error::Error>> {
    unsafe {
        let lib = Library::new(path)?;
        let version: extern "C" fn() -> u32 = *lib.get(b"oxidizer_api_version\0")?;
        if version() != oxidizer_api::API_VERSION {
            return Err(format!("built for API v{}, loader is v{}; rebuild the mod", version(), oxidizer_api::API_VERSION).into());
        }
        let init: extern "C" fn(&'static Api) = *lib.get(b"oxidizer_init\0")?;
        let m = RustMod {
            frame: lib.get(b"oxidizer_frame\0").ok().map(|s| *s),
            gui: lib.get(b"oxidizer_gui\0").ok().map(|s| *s),
            menu: lib.get(b"oxidizer_menu\0").ok().map(|s| *s),
            unload: lib.get(b"oxidizer_unload\0").ok().map(|s| *s),
        };
        std::mem::forget(lib); // mods stay loaded for the process lifetime (hooks may point into them)
        init(api);
        Ok(m)
    }
}
