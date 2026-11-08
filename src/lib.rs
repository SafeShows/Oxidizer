//! Oxidizer's core: everything that runs once .NET and the game are up. It is used two ways:
//! - by `oxidizer` (src/main.rs), the standalone launcher that starts the game's runtime itself, and
//! - as the `oxidizer_core` library that the bridge loads when Oxidizer runs as a StarMap mod (`Oxidizer.StarMap`),
//!   through `oxidizer_attach`.
//!
//! Either way the bridge's `Init` is handed in, the Api table is filled, and the mods are loaded.

mod bus;
mod ksa_rhai;
mod mods;
mod script;
mod ui;

use libloading::Library;
use oxidizer_api::{Api, imgui};
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock};
use std::collections::HashSet;
use std::fs;

/// `Bridge.Init` from bridge/Bridge.cs.
pub type BridgeInit = unsafe extern "C" fn(*mut Api, *const Host, *mut Internal) -> i32;

/// Bridge functions only Oxidizer itself uses (not part of the mod API). Must match `Internal` in bridge/Describe.cs.
#[repr(C)]
pub struct Internal {
    /// The game's public types and members, as text (see Describe.cs): fills buf, returns the full length.
    pub describe: extern "C" fn(*mut u8, usize) -> usize,
    /// 0 null, 1 bool, 2 integer or enum, 3 floating point, 4 string, 5 other object.
    pub kind: extern "C" fn(oxidizer_api::Handle) -> u8,
}

static INTERNAL: OnceLock<&'static Internal> = OnceLock::new();

fn internal() -> &'static Internal {
    INTERNAL.get().expect("bridge not attached")
}

/// Callbacks and settings for the bridge. Field order must match `Host` in `bridge/Bridge.cs`.
#[repr(C)]
pub struct Host {
    frame: extern "C" fn(f64, f64),
    gui: extern "C" fn(f64),
    menu: extern "C" fn(),
    /// UTF-8 instance folder, empty for the normal Documents folder.
    instance: *const u8,
    instance_len: usize,
    /// After the game loaded its content and mods.
    loaded: extern "C" fn(),
}

struct RustMod {
    frame: Option<extern "C" fn(f64, f64)>,
    gui: Option<extern "C" fn(f64)>,
    menu: Option<extern "C" fn()>,
    unload: Option<extern "C" fn()>,
    loaded: Option<extern "C" fn()>,
}

static RUST_MODS: OnceLock<Vec<RustMod>> = OnceLock::new();

pub fn log(s: &str) {
    println!("[oxidizer] {s}");
    ui::push_log(s.to_string());
}

extern "C" fn api_log(p: *const u8, n: usize) {
    log(&String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p, n) }));
}

/// Copies `s` into a caller's buffer (up to `cap` bytes) and returns its full length, as `read_string` expects.
pub(crate) fn write_out(s: &str, buf: *mut u8, cap: usize) -> usize {
    let n = s.len().min(cap);
    unsafe { std::ptr::copy_nonoverlapping(s.as_ptr(), buf, n) };
    s.len()
}

extern "C" fn api_log_next() -> u64 {
    ui::log_next()
}

extern "C" fn api_log_read(from: u64, buf: *mut u8, cap: usize) -> usize {
    write_out(&ui::log_from(from), buf, cap)
}

extern "C" fn api_mod_status(buf: *mut u8, cap: usize) -> usize {
    write_out(&ui::status_text(), buf, cap)
}

thread_local! {
    /// The value or error of the last `rhai_eval` on this thread, for `rhai_result`.
    static EVAL_RESULT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

extern "C" fn api_rhai_eval(p: *const u8, n: usize) -> u8 {
    let code = String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p, n) }).into_owned();
    let result = std::panic::catch_unwind(|| script::eval(&code)).unwrap_or_else(|_| Err("the script engine panicked".into()));
    let ok = result.is_ok();
    EVAL_RESULT.with_borrow_mut(|r| *r = result.unwrap_or_else(|e| e));
    ok as u8
}

extern "C" fn api_rhai_result(buf: *mut u8, cap: usize) -> usize {
    EVAL_RESULT.with_borrow(|r| write_out(r, buf, cap))
}

/// Whether the game has finished loading: script mods that start later get loaded() right after init().
pub(crate) static LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn on_loaded() {
    if LOADED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return; // once, even if the game loads its mods again
    }
    log("game loaded");
    for m in RUST_MODS.get().into_iter().flatten() {
        m.loaded.map(|f| f());
    }
    script::call_all("loaded", ());
}

extern "C" fn on_frame(t: f64, dt: f64) {
    script::reload_changed();
    bus::deliver(); // events published since the last frame, before mods' frame()
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

/// Set up the bridge (`instance`: folder to use instead of the game's documents folder, or "") and load the mods.
///
/// # Safety
/// `bridge_init` must be the bridge's `Init`, called once, before the game's `Main`.
pub unsafe fn attach(bridge_init: BridgeInit, instance: &str) -> Result<(), String> {
    let api: &'static mut MaybeUninit<Api> = Box::leak(Box::new(MaybeUninit::uninit()));
    let internals: &'static mut MaybeUninit<Internal> = Box::leak(Box::new(MaybeUninit::uninit()));
    let instance: &'static str = Box::leak(instance.into());
    unsafe {
        (&raw mut (*api.as_mut_ptr()).log).write(api_log);
        let host = Host { frame: on_frame, gui: on_gui, menu: on_menu, instance: instance.as_ptr(), instance_len: instance.len(), loaded: on_loaded };
        if bridge_init(api.as_mut_ptr(), &host, internals.as_mut_ptr()) == 0 {
            return Err("bridge init failed".into());
        }
        let _ = INTERNAL.set(internals.assume_init_ref());
        // API version 3: the loader's own functions, after the bridge's part of the table.
        let p = api.as_mut_ptr();
        (&raw mut (*p).log_next).write(api_log_next);
        (&raw mut (*p).log_read).write(api_log_read);
        (&raw mut (*p).mod_status).write(api_mod_status);
        (&raw mut (*p).rhai_eval).write(api_rhai_eval);
        (&raw mut (*p).rhai_result).write(api_rhai_result);
        // API version 5: the bus and services. The loader's own table has no mod id; each Rust mod gets a copy with its own.
        (&raw mut (*p).mod_id).write(std::ptr::null());
        (&raw mut (*p).mod_id_len).write(0);
        (&raw mut (*p).last_text).write(bus::api_last_text);
        (&raw mut (*p).bus_publish).write(bus::api_publish);
        (&raw mut (*p).bus_subscribe).write(bus::api_subscribe);
        (&raw mut (*p).bus_unsubscribe).write(bus::api_unsubscribe);
        (&raw mut (*p).service_provide).write(bus::api_provide);
        (&raw mut (*p).service_reply).write(bus::api_reply);
        (&raw mut (*p).service_call).write(bus::api_call);
        let api: &'static Api = api.assume_init_ref();
        oxidizer_api::set_api(api);
        load_mods(api);
    }
    Ok(())
}

/// Let mods know the game is closing and save their settings. Runs once.
pub fn shutdown() {
    static DONE: Once = Once::new();
    DONE.call_once(|| {
        for m in RUST_MODS.get().into_iter().flatten() {
            m.unload.map(|f| f());
        }
        script::call_all("unload", ());
        script::save_settings();
    });
}

/// Entry point when the bridge runs as a StarMap mod: StarMap already started .NET (and handles instances).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oxidizer_attach(bridge_init: BridgeInit) -> i32 {
    match std::panic::catch_unwind(|| unsafe { attach(bridge_init, "") }) {
        Ok(Ok(())) => 1,
        Ok(Err(e)) => {
            log(&e);
            0
        }
        Err(_) => {
            log("oxidizer_attach panicked");
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oxidizer_shutdown() {
    let _ = std::panic::catch_unwind(shutdown);
}

/// Mod folder = `<Documents>/My Games/Kitten Space Agency/mods/<id>/mod.toml` with `[oxidizer] dll = "x"` and/or
/// `script = "main.rhai"`. Mods start in dependency order (see mods.rs), Rust and Rhai mods alike, and a mod whose
/// dependencies aren't met doesn't start. Mods disabled in manifest.toml are skipped.
fn load_mods(api: &'static Api) {
    let docs = PathBuf::from(oxidizer_api::read_string(|b, c| (api.docs_path)(b, c)));
    let mods_dir = docs.join("mods");
    if !mods_dir.is_dir() {
        return log(&format!("no mods folder in {}", docs.display()));
    }
    let disabled = mods::disabled_mods(&docs.join("manifest.toml"));
    for id in fs::read_dir(&mods_dir).into_iter().flatten().flatten().filter_map(|e| mods::read(&e.path())).map(|e| e.id) {
        if disabled.contains(&id) {
            log(&format!("{id}: disabled in manifest"));
            ui::set_status(&id, "mod", Some("disabled in the game's mod list".into()));
        }
    }
    let (ordered, cycle) = mods::order(mods::scan(&mods_dir));
    if !cycle.is_empty() {
        log(&format!("dependency cycle between {}: starting them in folder order", cycle.join(", ")));
    }
    let oxidizer: HashSet<String> = ordered.iter().map(|e| e.id.clone()).collect();
    let mut rust = vec![];
    for e in &ordered {
        if let Err(why) = mods::check(e, &mods_dir, &oxidizer) {
            mods::mark_waiting(e, why);
            continue;
        }
        let mut ok = true;
        if let Some(dll) = &e.dll {
            match unsafe { load_rust_mod(&mod_library(&e.dir, dll), api, &e.id) } {
                Ok(m) => {
                    log(&format!("loaded rust mod {}", e.id));
                    ui::set_status(&e.id, "Rust", None);
                    rust.push(m);
                }
                Err(err) => {
                    log(&format!("{}: {err}", e.id));
                    ui::set_status(&e.id, "Rust", Some(err.to_string()));
                    ok = false;
                }
            }
        }
        if let Some(script) = &e.script {
            ok &= script::start_mod(&e.id, script);
        }
        if ok {
            mods::mark_started(e);
        }
    }
    let _ = RUST_MODS.set(rust);
}

/// `dll` as written if that file exists, otherwise this platform's library file for its stem in the same folder,
/// so `dll = "bin/my_mod"` finds `bin/my_mod.dll` on Windows, `bin/libmy_mod.so` on Linux, `bin/libmy_mod.dylib` on macOS.
fn mod_library(dir: &Path, dll: &str) -> PathBuf {
    let exact = dir.join(dll);
    if exact.exists() {
        return exact;
    }
    let folder = exact.parent().unwrap_or(dir);
    let stem = exact.file_stem().unwrap_or(dll.as_ref());
    folder.join(libloading::library_filename(stem))
}

/// Loads a Rust mod and runs its init, giving it its own copy of the API table with its id, so the bus knows who
/// calls (also from hooks, outside any Oxidizer callback).
unsafe fn load_rust_mod(path: &Path, api: &'static Api, id: &str) -> Result<RustMod, Box<dyn std::error::Error>> {
    unsafe {
        let lib = Library::new(path)?;
        let version: extern "C" fn() -> u32 = *lib.get(b"oxidizer_api_version\0")?;
        // Older mods only use the start of the table, which never changes; newer ones need a newer loader.
        if version() == 0 || version() > oxidizer_api::API_VERSION {
            return Err(format!("built for API v{}, this Oxidizer supports up to v{}; update Oxidizer", version(), oxidizer_api::API_VERSION).into());
        }
        let init: extern "C" fn(&'static Api) = *lib.get(b"oxidizer_init\0")?;
        let m = RustMod {
            frame: lib.get(b"oxidizer_frame\0").ok().map(|s| *s),
            gui: lib.get(b"oxidizer_gui\0").ok().map(|s| *s),
            menu: lib.get(b"oxidizer_menu\0").ok().map(|s| *s),
            unload: lib.get(b"oxidizer_unload\0").ok().map(|s| *s),
            loaded: lib.get(b"oxidizer_loaded\0").ok().map(|s| *s),
        };
        std::mem::forget(lib); // mods stay loaded for the process lifetime (hooks may point into them)
        let id: &'static str = Box::leak(id.into());
        let mut table = std::ptr::read(api);
        table.mod_id = id.as_ptr();
        table.mod_id_len = id.len();
        init(Box::leak(Box::new(table)));
        Ok(m)
    }
}
