//! Rust mods: loading them, calling them, and hot reload.
//!
//! Each mod runs from a copy of its library (in `<documents>/oxidizer/loaded/`), so the file in the mod's folder
//! can be rebuilt while the game runs. About once a second Oxidizer checks those files; when one changed (and has
//! stopped changing, so a copy in progress isn't read), the mod's `unload` runs, its hooks, event subscriptions
//! and services are removed, and the new library starts with `init` (and `loaded`, if the game has loaded).
//!
//! The old copy stays loaded: anything still pointing into it (a thread the mod started, a hook added from
//! outside its callbacks) keeps working with the old code instead of crashing the game.

use crate::{LOADED, bus, internal, log, ui};
use libloading::Library;
use oxidizer_api::{Api, HookFn};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime};

/// A library file's modification time and size: when it differs from the loaded one, the mod was rebuilt.
type Stamp = (SystemTime, u64);

#[derive(Clone)]
struct RustMod {
    id: &'static str,
    /// The library in the mod's folder (Oxidizer runs a copy).
    source: PathBuf,
    /// The source as it was when loaded, and as it was at the last check if it changed since.
    stamp: Option<Stamp>,
    pending: Option<Stamp>,
    init: extern "C" fn(&'static Api),
    frame: Option<extern "C" fn(f64, f64)>,
    gui: Option<extern "C" fn(f64)>,
    menu: Option<extern "C" fn()>,
    unload: Option<extern "C" fn()>,
    loaded: Option<extern "C" fn()>,
}

static API: OnceLock<&'static Api> = OnceLock::new();
static COPIES: OnceLock<PathBuf> = OnceLock::new();
static MODS: Mutex<Vec<RustMod>> = Mutex::new(Vec::new());
/// A Rust mod's hook, as the bridge calls it: through `timed_hook`, which times it for the Performance view.
struct Hook {
    id: &'static str,
    cb: HookFn,
    user: usize,
}

/// The hooks Rust mods added (leaked `Hook`s, as the bridge's user values), so a reload can take a mod's out.
static HOOKS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static NEXT_COPY: AtomicU64 = AtomicU64::new(1);
const CHECK_EVERY: Duration = Duration::from_secs(1);

thread_local! {
    /// The Rust mod Oxidizer is calling on this thread: hooks it adds are its own.
    static RUNNING: Cell<Option<&'static str>> = const { Cell::new(None) };
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Calls into a mod, marking it as the one running, and timing it.
fn run(id: &'static str, f: impl FnOnce()) {
    let outer = RUNNING.replace(Some(id));
    crate::perf::time(id, f);
    RUNNING.set(outer);
}

/// Where the copies go: a folder per game process (another running game may hold its copies open).
pub fn setup(api: &'static Api, docs: &Path) {
    let _ = API.set(api);
    let root = docs.join("oxidizer").join("loaded");
    for old in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        let _ = std::fs::remove_dir_all(old.path()); // earlier runs; fails harmlessly for a game still running
    }
    let dir = root.join(std::process::id().to_string());
    let _ = std::fs::create_dir_all(&dir);
    let _ = COPIES.set(dir);
}

fn stamp(path: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// Copies the library and opens the copy, checking it was built for an API this Oxidizer has.
fn open(id: &'static str, source: &Path) -> Result<RustMod, String> {
    let stamp = stamp(source);
    let ext = source.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let copy = COPIES.get().ok_or("not set up")?.join(format!("{id}-{}{ext}", NEXT_COPY.fetch_add(1, Ordering::Relaxed)));
    std::fs::copy(source, &copy).map_err(|e| format!("can't copy {}: {e}", source.display()))?;
    unsafe {
        let lib = Library::new(&copy).map_err(|e| e.to_string())?;
        let version: extern "C" fn() -> u32 = *lib.get(b"oxidizer_api_version\0").map_err(|e| e.to_string())?;
        // Older mods only use the start of the table, which never changes; newer ones need a newer loader.
        if version() == 0 || version() > oxidizer_api::API_VERSION {
            return Err(format!("built for API v{}, this Oxidizer supports up to v{}; update Oxidizer", version(), oxidizer_api::API_VERSION));
        }
        let m = RustMod {
            id,
            source: source.to_path_buf(),
            stamp,
            pending: None,
            init: *lib.get(b"oxidizer_init\0").map_err(|e| e.to_string())?,
            frame: lib.get(b"oxidizer_frame\0").ok().map(|s| *s),
            gui: lib.get(b"oxidizer_gui\0").ok().map(|s| *s),
            menu: lib.get(b"oxidizer_menu\0").ok().map(|s| *s),
            unload: lib.get(b"oxidizer_unload\0").ok().map(|s| *s),
            loaded: lib.get(b"oxidizer_loaded\0").ok().map(|s| *s),
        };
        std::mem::forget(lib); // copies stay loaded for the process lifetime (see the top of this file)
        Ok(m)
    }
}

/// Runs a mod's init with its own copy of the API table: its id (so the bus knows who calls, also from hooks) and
/// a `patch` that remembers its hooks.
fn begin(m: &RustMod) {
    let mut table = unsafe { std::ptr::read(*API.get().expect("not set up")) };
    table.mod_id = m.id.as_ptr();
    table.mod_id_len = m.id.len();
    table.patch = api_patch;
    let table: &'static Api = Box::leak(Box::new(table));
    run(m.id, || (m.init)(table));
}

extern "C" fn api_patch(ty: *const u8, tn: usize, me: *const u8, mn: usize, post: u8, cb: HookFn, user: usize) -> u8 {
    // ponytail: a hook added outside the mod's callbacks (from a thread, say) is timed as "unknown" and stays
    // through a reload, running the old copy's code.
    let hook = Box::into_raw(Box::new(Hook { id: RUNNING.get().unwrap_or("unknown"), cb, user })) as usize;
    let ok = (API.get().expect("not set up").patch)(ty, tn, me, mn, post, timed_hook, hook);
    if ok == 0 {
        drop(unsafe { Box::from_raw(hook as *mut Hook) });
    } else if RUNNING.get().is_some() {
        lock(&HOOKS).push(hook);
    }
    ok
}

extern "C" fn timed_hook(hook: usize, instance: oxidizer_api::Handle, args: oxidizer_api::Handle) -> u8 {
    let h = unsafe { &*(hook as *const Hook) };
    let mut r = 1;
    run(h.id, || r = (h.cb)(h.user, instance, args));
    r
}

/// Loads and starts a mod (in load order). The error says why it didn't.
pub fn start(id: &str, source: &Path) -> Result<(), String> {
    let id: &'static str = Box::leak(id.into());
    let m = open(id, source)?;
    begin(&m);
    lock(&MODS).push(m);
    Ok(())
}

/// The mods, copied out: no lock is held while they run (a mod may be reloaded meanwhile).
fn mods() -> Vec<RustMod> {
    lock(&MODS).clone()
}

pub fn call_loaded() {
    mods().into_iter().for_each(|m| m.loaded.into_iter().for_each(|f| run(m.id, || f())));
}

pub fn call_frame(t: f64, dt: f64) {
    mods().into_iter().for_each(|m| m.frame.into_iter().for_each(|f| run(m.id, || f(t, dt))));
}

pub fn call_gui(dt: f64) {
    mods().into_iter().for_each(|m| m.gui.into_iter().for_each(|f| run(m.id, || f(dt))));
}

pub fn call_menu() {
    mods().into_iter().for_each(|m| m.menu.into_iter().for_each(|f| run(m.id, || f())));
}

pub fn call_unload() {
    mods().into_iter().for_each(|m| m.unload.into_iter().for_each(|f| run(m.id, || f())));
}

/// Reloads the mods whose library was rebuilt. Checks about once a second; a file must look the same two checks in
/// a row, so one still being copied isn't loaded half-written.
pub fn reload_changed() {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut last = lock(&LAST);
        if last.is_some_and(|t| t.elapsed() < CHECK_EVERY) {
            return;
        }
        *last = Some(Instant::now());
    }
    let ready: Vec<RustMod> = {
        let mut mods = lock(&MODS);
        let mut ready = vec![];
        for m in mods.iter_mut() {
            let now = stamp(&m.source);
            if now.is_none() || now == m.stamp {
                m.pending = None;
            } else if now == m.pending {
                m.stamp = now; // tried once per change, even if the new library doesn't load
                m.pending = None;
                ready.push(m.clone());
            } else {
                m.pending = now;
            }
        }
        ready
    };
    for old in ready {
        reload(old);
    }
}

fn reload(old: RustMod) {
    let id = old.id;
    // The new library is opened first: if it can't load, the running one stays.
    let new = match open(id, &old.source) {
        Ok(new) => new,
        Err(e) => return log(&format!("{id}: rebuilt, but not reloaded: {e}")),
    };
    old.unload.into_iter().for_each(|f| run(id, || f()));
    forget(id);
    begin(&new);
    if LOADED.load(Ordering::Relaxed) {
        new.loaded.into_iter().for_each(|f| run(id, || f()));
    }
    if let Some(slot) = lock(&MODS).iter_mut().find(|m| m.id == id) {
        *slot = new;
    }
    log(&format!("reloaded rust mod {id}"));
    ui::set_status(id, "Rust", None);
}

/// Takes out what a mod added: its hooks, event subscriptions and services.
fn forget(id: &str) {
    let hooks: Vec<usize> = {
        let mut all = lock(&HOOKS);
        let (mine, rest) = all.iter().partition(|&&h| unsafe { &*(h as *const Hook) }.id == id);
        *all = rest;
        mine
    };
    for hook in hooks {
        (internal().unhook)(timed_hook as *const () as usize, hook); // the Hook stays allocated: the bridge may be running it
    }
    bus::forget(id);
}
