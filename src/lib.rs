//! Oxidizer's core: everything that runs once .NET and the game are up. It is used two ways:
//! - by `oxidizer` (src/main.rs), the standalone launcher that starts the game's runtime itself, and
//! - as the `oxidizer_core` library that the bridge loads when Oxidizer runs as a StarMap mod (`Oxidizer.StarMap`),
//!   through `oxidizer_attach`.
//!
//! Either way the bridge's `Init` is handed in, the Api table is filled, and the mods are loaded.

mod ksa_rhai;
mod script;
mod ui;

use libloading::Library;
use oxidizer_api::{Api, imgui};
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock};
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
        let host = Host { frame: on_frame, gui: on_gui, menu: on_menu, instance: instance.as_ptr(), instance_len: instance.len() };
        if bridge_init(api.as_mut_ptr(), &host, internals.as_mut_ptr()) == 0 {
            return Err("bridge init failed".into());
        }
        let _ = INTERNAL.set(internals.assume_init_ref());
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

/// Mod folder = `<Documents>/My Games/Kitten Space Agency/mods/<id>/mod.toml` with
/// `[oxidizer] dll = "x"` and/or `script = "main.rhai"`. Respects `enabled = false` in manifest.toml.
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
        };
        std::mem::forget(lib); // mods stay loaded for the process lifetime (hooks may point into them)
        init(api);
        Ok(m)
    }
}
