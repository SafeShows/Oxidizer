//! API for KSA Rust mods. Game objects are opaque .NET GC handles wrapped in [`Obj`];
//! members are reached by name through reflection in the C# bridge.
//!
//! ```ignore
//! struct MyMod;
//! impl oxidizer_api::Mod for MyMod { fn init() -> Self { oxidizer_api::log("hi"); MyMod } }
//! oxidizer_api::oxidizer_mod!(MyMod);
//! ```

use std::fmt;
use std::sync::OnceLock;

pub mod bind;
pub mod bus;
pub mod imgui;
pub mod physics;
pub mod save;

pub use bus::{Event, Service, Subscription, call, call_json, provide, provide_json, publish, publish_json, publish_retained, publish_retained_json, subscribe, subscribe_json};
/// Re-exported for `#[derive(Serialize, Deserialize)]` on event and service types (`oxidizer_api::serde`).
pub use serde;
pub use serde_json;

/// ABI version of [`Api`] and the mod exports. Bump when the table grows or changes;
/// the loader runs mods built for its version or an older one, and refuses newer ones.
/// Version 2 added hook results, keys, typed-binding calls and 64-bit integers.
/// Version 3 added reading the log and mod status, and running Rhai code.
/// Version 4 added `Mod::loaded`.
/// Version 5 added the event bus and services, and gave each mod its own table with its id.
/// Version 6 added physics forces ([`physics`]), per-save data ([`save`]) and `Mod::saving` / `Mod::save_loaded`.
pub const API_VERSION: u32 = 6;

/// .NET GCHandle. 0 = null.
pub type Handle = usize;
/// Hook callback: (user data, instance or 0 for static methods, object[] of args) -> 0 to skip the original (prefix only).
/// The callback owns both handles (wrap with [`Obj::from_handle`]).
pub type HookFn = extern "C" fn(user: usize, instance: Handle, args: Handle) -> u8;

/// Function table shared by loader, bridge and mods. Field order must match `bridge/Bridge.cs`.
#[repr(C)]
pub struct Api {
    pub log: extern "C" fn(*const u8, usize),
    pub free: extern "C" fn(Handle),
    /// (obj or 0 for static, type name, member) -> value
    pub get: extern "C" fn(Handle, *const u8, usize, *const u8, usize) -> Handle,
    pub set: extern "C" fn(Handle, *const u8, usize, *const u8, usize, Handle) -> u8,
    pub call: extern "C" fn(Handle, *const u8, usize, *const u8, usize, *const Handle, usize) -> Handle,
    pub box_f64: extern "C" fn(f64) -> Handle,
    pub box_str: extern "C" fn(*const u8, usize) -> Handle,
    pub as_f64: extern "C" fn(Handle, *mut f64) -> u8,
    /// Writes UTF-8 of ToString() into buf (up to cap), returns full length.
    pub as_str: extern "C" fn(Handle, *mut u8, usize) -> usize,
    /// Count of an array/collection, -1 otherwise.
    pub len: extern "C" fn(Handle) -> i64,
    pub index: extern "C" fn(Handle, i64) -> Handle,
    /// (type, method, postfix?, callback, user) -> ok
    pub patch: extern "C" fn(*const u8, usize, *const u8, usize, u8, HookFn, usize) -> u8,
    pub docs_path: extern "C" fn(*mut u8, usize) -> usize,
    // ---- API version 2 ----
    /// Return value of the hooked method (inside a hook only), else 0.
    pub result_get: extern "C" fn() -> Handle,
    /// Replace the hooked method's return value (inside a hook only).
    pub result_set: extern "C" fn(Handle) -> u8,
    /// Key (game key name such as "F5") pressed this frame.
    pub key_pressed: extern "C" fn(*const u8, usize) -> u8,
    /// Key held down.
    pub key_down: extern "C" fn(*const u8, usize) -> u8,
    /// Call a KSA.dll method/constructor by metadata token: (obj or 0 for static, token, args, n) -> result.
    pub invoke: extern "C" fn(Handle, i32, *const Handle, usize) -> Handle,
    /// Read a KSA.dll field by metadata token.
    pub field_get: extern "C" fn(Handle, i32) -> Handle,
    pub field_set: extern "C" fn(Handle, i32, Handle) -> u8,
    /// The installed KSA.dll build (module version id).
    pub ksa_version: extern "C" fn(*mut u8, usize) -> usize,
    pub as_i64: extern "C" fn(Handle, *mut i64) -> u8,
    pub box_i64: extern "C" fn(i64) -> Handle,
    // ---- API version 3 (filled by the loader itself, not the bridge) ----
    /// Number the next log line will get (log lines are numbered from 0).
    pub log_next: extern "C" fn() -> u64,
    /// Log lines from number `from` on that are still kept, joined by '\n'.
    pub log_read: extern "C" fn(u64, *mut u8, usize) -> usize,
    /// Every Oxidizer mod: "id\tkind\tproblem" lines, problem empty while it runs.
    pub mod_status: extern "C" fn(*mut u8, usize) -> usize,
    /// Run Rhai code in Oxidizer's script engine: 1 when it ran; [`Api::rhai_result`] then has the value, else the error.
    pub rhai_eval: extern "C" fn(*const u8, usize) -> u8,
    pub rhai_result: extern "C" fn(*mut u8, usize) -> usize,
    // ---- API version 5 (filled by the loader) ----
    /// The id (folder name) of the mod this table was given to: each Rust mod gets its own copy of the table.
    /// Empty in the loader's own table.
    pub mod_id: *const u8,
    pub mod_id_len: usize,
    /// The text of the last bus or service call on this thread that failed (the error), or a service call's result.
    pub last_text: extern "C" fn(*mut u8, usize) -> usize,
    /// (caller, topic, JSON value, retained) -> 1 when queued.
    pub bus_publish: extern "C" fn(*const u8, usize, *const u8, usize, *const u8, usize, u8) -> u8,
    /// (caller, topic, callback, user) -> subscription id, or 0 on error.
    pub bus_subscribe: extern "C" fn(*const u8, usize, *const u8, usize, BusFn, usize) -> u64,
    pub bus_unsubscribe: extern "C" fn(u64),
    /// (caller, service name, callback, user) -> 1 when provided.
    pub service_provide: extern "C" fn(*const u8, usize, *const u8, usize, ServiceFn, usize) -> u8,
    /// Inside a service callback: its JSON result, or the error when the callback returns 0.
    pub service_reply: extern "C" fn(*const u8, usize),
    /// (caller, service name, JSON arguments) -> 1 when it returned; `last_text` then has the JSON result, else the error.
    pub service_call: extern "C" fn(*const u8, usize, *const u8, usize, *const u8, usize) -> u8,
    // ---- API version 6 (filled by the loader) ----
    /// (callback, user) -> 1 when registered and the game's physics is patched. See [`physics`].
    pub force_register: extern "C" fn(physics::ForceFn, usize) -> u8,
    /// Key of a `KSA.Vehicle` as force callbacks see it, 0 for null.
    pub vehicle_key: extern "C" fn(Handle) -> u64,
    /// (mod id, key) -> 1 and the value as JSON in `last_text`, or 0 when there is none. See [`save`].
    pub save_get: extern "C" fn(*const u8, usize, *const u8, usize) -> u8,
    /// (mod id, key, JSON value; `null` removes) -> 0 when the JSON isn't valid.
    pub save_set: extern "C" fn(*const u8, usize, *const u8, usize, *const u8, usize) -> u8,
}

// The table only holds function pointers and a pointer to a string that lives as long as the process.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

/// Event callback: (user data, JSON value, its length).
pub type BusFn = extern "C" fn(user: usize, json: *const u8, len: usize);
/// Service callback: (user data, JSON arguments, their length) -> 1 when it worked. It sets its result (or error)
/// with [`Api::service_reply`].
pub type ServiceFn = extern "C" fn(user: usize, args: *const u8, len: usize) -> u8;

static API: OnceLock<&'static Api> = OnceLock::new();

#[doc(hidden)]
pub fn set_api(api: &'static Api) {
    let _ = API.set(api);
}

pub fn api() -> &'static Api {
    API.get().expect("oxidizer_api used before oxidizer_init")
}

pub fn log(s: &str) {
    (api().log)(s.as_ptr(), s.len())
}

/// Read a UTF-8 string out of a bridge "fill buffer, return length" call.
pub fn read_string(f: impl Fn(*mut u8, usize) -> usize) -> String {
    let mut buf = vec![0u8; 256];
    let n = f(buf.as_mut_ptr(), buf.len());
    if n > buf.len() {
        buf.resize(n, 0);
        f(buf.as_mut_ptr(), n);
    }
    buf.truncate(n);
    String::from_utf8_lossy(&buf).into_owned()
}

/// Owned reference to a .NET object (freed on drop).
pub struct Obj(Handle);

impl Obj {
    pub const NULL: Obj = Obj(0);

    /// # Safety
    /// `h` must be an owned handle from the bridge (or 0).
    pub unsafe fn from_handle(h: Handle) -> Obj {
        Obj(h)
    }

    pub fn handle(&self) -> Handle {
        self.0
    }

    pub fn is_null(&self) -> bool {
        self.0 == 0
    }

    /// Static field/property, e.g. `Obj::static_get("KSA.Program", "ControlledVehicle")`.
    /// Type names may be assembly-qualified ("KSA.Program, KSA").
    pub fn static_get(ty: &str, member: &str) -> Obj {
        Obj((api().get)(0, ty.as_ptr(), ty.len(), member.as_ptr(), member.len()))
    }

    pub fn get(&self, member: &str) -> Obj {
        Obj((api().get)(self.0, std::ptr::null(), 0, member.as_ptr(), member.len()))
    }

    pub fn set(&self, member: &str, value: &Obj) -> bool {
        (api().set)(self.0, std::ptr::null(), 0, member.as_ptr(), member.len(), value.0) != 0
    }

    pub fn static_set(ty: &str, member: &str, value: &Obj) -> bool {
        (api().set)(0, ty.as_ptr(), ty.len(), member.as_ptr(), member.len(), value.0) != 0
    }

    /// Calls the first method with this name and arg count; args are converted to parameter types.
    pub fn call(&self, method: &str, args: &[&Obj]) -> Obj {
        Self::call_raw(self.0, "", method, args)
    }

    pub fn static_call(ty: &str, method: &str, args: &[&Obj]) -> Obj {
        Self::call_raw(0, ty, method, args)
    }

    fn call_raw(h: Handle, ty: &str, method: &str, args: &[&Obj]) -> Obj {
        let hs: Vec<Handle> = args.iter().map(|a| a.0).collect();
        Obj((api().call)(h, ty.as_ptr(), ty.len(), method.as_ptr(), method.len(), hs.as_ptr(), hs.len()))
    }

    /// Integer/enum value as i64 (exact, unlike [`Obj::f64`] for large numbers).
    pub fn i64(&self) -> Option<i64> {
        let mut v = 0;
        ((api().as_i64)(self.0, &mut v) != 0).then_some(v)
    }

    /// Numeric/bool/enum value as f64.
    pub fn f64(&self) -> Option<f64> {
        let mut v = 0.0;
        ((api().as_f64)(self.0, &mut v) != 0).then_some(v)
    }

    pub fn len(&self) -> Option<usize> {
        usize::try_from((api().len)(self.0)).ok()
    }

    pub fn index(&self, i: usize) -> Obj {
        Obj((api().index)(self.0, i as i64))
    }
}

impl Drop for Obj {
    fn drop(&mut self) {
        if self.0 != 0 {
            (api().free)(self.0)
        }
    }
}

/// `ToString()` of the .NET object ("null" for null).
impl fmt::Display for Obj {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&read_string(|b, c| (api().as_str)(self.0, b, c)))
    }
}

impl From<f64> for Obj {
    fn from(v: f64) -> Obj {
        Obj((api().box_f64)(v))
    }
}

impl From<i64> for Obj {
    fn from(v: i64) -> Obj {
        Obj((api().box_i64)(v))
    }
}

impl From<bool> for Obj {
    fn from(v: bool) -> Obj {
        Obj((api().box_f64)(v as u8 as f64))
    }
}

impl From<&str> for Obj {
    fn from(s: &str) -> Obj {
        Obj((api().box_str)(s.as_ptr(), s.len()))
    }
}

/// Harmony-patch `ty.method` with a raw callback. See [`HookFn`].
pub fn patch(ty: &str, method: &str, postfix: bool, cb: HookFn, user: usize) -> bool {
    (api().patch)(ty.as_ptr(), ty.len(), method.as_ptr(), method.len(), postfix as u8, cb, user) != 0
}

/// Patch with a plain fn(instance, args) -> run_original. Return value is ignored for postfixes.
pub fn hook(ty: &str, method: &str, postfix: bool, f: fn(&Obj, &Obj) -> bool) -> bool {
    extern "C" fn tramp(user: usize, inst: Handle, args: Handle) -> u8 {
        let f: fn(&Obj, &Obj) -> bool = unsafe { std::mem::transmute(user) };
        let (inst, args) = (Obj(inst), Obj(args));
        std::panic::catch_unwind(|| f(&inst, &args)).unwrap_or(true) as u8
    }
    patch(ty, method, postfix, tramp, f as usize)
}

/// Inside a hook: the hooked method's return value (null for void methods or outside a hook).
/// In a prefix it is the default until something sets it.
pub fn hook_result() -> Obj {
    Obj((api().result_get)())
}

/// Inside a hook: replace the hooked method's return value (converted to its return type).
/// From a prefix, combine with returning `false` to skip the original and return this value.
pub fn set_hook_result(value: &Obj) -> bool {
    (api().result_set)(value.0) != 0
}

/// True on the frame `key` was pressed. Key names are the game's (GLFW) names, e.g. "F5", "K", "Space".
/// Keys typed into text fields or the console don't count.
pub fn key_pressed(key: &str) -> bool {
    (api().key_pressed)(key.as_ptr(), key.len()) != 0
}

/// True while `key` is held down.
pub fn key_down(key: &str) -> bool {
    (api().key_down)(key.as_ptr(), key.len()) != 0
}

/// Where a mod keeps its settings: `<game documents>/oxidizer/<mod_id>.toml` (its folder is created).
/// It lives outside the mod folder so updating the mod doesn't lose it; read and write it with your own TOML code.
pub fn config_path(mod_id: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(read_string(|b, c| (api().docs_path)(b, c))).join("oxidizer");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{mod_id}.toml"))
}

/// The game's mods folder, `<game documents>/mods`, where every mod has its folder.
pub fn mods_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(read_string(|b, c| (api().docs_path)(b, c))).join("mods")
}

/// Number the next line of Oxidizer's log will get. Lines are numbered from 0 as they are logged; keep this to
/// read only what is new with [`log_lines`].
pub fn log_next() -> u64 {
    (api().log_next)()
}

/// Oxidizer's log (what **Mods > Oxidizer** shows) from line number `from` on. Only the last few hundred lines
/// are kept, so older ones are gone.
pub fn log_lines(from: u64) -> Vec<String> {
    let text = read_string(|b, c| (api().log_read)(from, b, c));
    if text.is_empty() { vec![] } else { text.split('\n').map(String::from).collect() }
}

/// One Oxidizer mod, as **Mods > Oxidizer** shows it.
#[derive(Clone, Debug)]
pub struct ModStatus {
    /// Its folder name in the mods folder.
    pub id: String,
    /// "Rust" or "Rhai".
    pub kind: String,
    /// Why it isn't running (failed to load, or stopped after an error), or None while it runs.
    pub problem: Option<String>,
}

pub fn mod_statuses() -> Vec<ModStatus> {
    read_string(|b, c| (api().mod_status)(b, c))
        .lines()
        .filter_map(|l| {
            let mut f = l.splitn(3, '\t');
            let (id, kind, problem) = (f.next()?, f.next()?, f.next().unwrap_or(""));
            Some(ModStatus { id: id.into(), kind: kind.into(), problem: (!problem.is_empty()).then(|| problem.into()) })
        })
        .collect()
}

/// Runs Rhai code in Oxidizer's script engine, with the game access Rhai mods have, and returns its value as text
/// (empty for `()`), or the error. Variables persist between calls, like a console.
pub fn rhai_eval(code: &str) -> Result<String, String> {
    let ok = (api().rhai_eval)(code.as_ptr(), code.len()) != 0;
    let result = read_string(|b, c| (api().rhai_result)(b, c));
    if ok { Ok(result) } else { Err(result) }
}

/// Implement and register with [`oxidizer_mod!`].
pub trait Mod: Send + 'static {
    /// When the mod starts: before the game's `Main`, so before the game has loaded anything.
    fn init() -> Self;
    /// Once the game has loaded its content and mods (after `KSA.ModLibrary.LoadAll`): the place for setup that
    /// needs the game's data.
    fn loaded(&mut self) {}
    /// After `KSA.Program.OnFrame`.
    fn frame(&mut self, _player_time: f64, _dt: f64) {}
    /// Inside the ImGui frame; draw windows here with [`imgui`].
    fn gui(&mut self, _dt: f64) {}
    /// Inside the "Mods" menu of the game's menu bar; add entries with [`imgui::menu_item`].
    fn menu(&mut self) {}
    /// When the game closes, and before a hot reload replaces this mod with a new build: save what should survive,
    /// and stop threads the mod started.
    fn unload(&mut self) {}
    /// The game is about to write a save: put what this save should remember into it with [`save::set`].
    fn saving(&mut self) {}
    /// The game loaded a save: read what it remembered with [`save::get`].
    fn save_loaded(&mut self) {}
}

#[doc(hidden)]
pub fn guard(name: &str, f: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
        log(&format!("mod panicked in {name}"));
    }
}

/// Exports the C entry points the loader looks for.
#[macro_export]
macro_rules! oxidizer_mod {
    ($t:ty) => {
        static __OXIDIZER_MOD: ::std::sync::Mutex<Option<$t>> = ::std::sync::Mutex::new(None);
        #[unsafe(no_mangle)]
        pub extern "C" fn oxidizer_api_version() -> u32 {
            $crate::API_VERSION
        }
        fn __oxidizer_with(name: &str, f: impl FnOnce(&mut $t)) {
            $crate::guard(name, || {
                if let Some(m) = __OXIDIZER_MOD.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                    f(m)
                }
            })
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn oxidizer_init(api: &'static $crate::Api) {
            $crate::set_api(api);
            $crate::guard("init", || {
                let m = <$t as $crate::Mod>::init();
                *__OXIDIZER_MOD.lock().unwrap_or_else(|e| e.into_inner()) = Some(m);
            })
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn oxidizer_frame(t: f64, dt: f64) {
            __oxidizer_with("frame", |m| $crate::Mod::frame(m, t, dt))
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn oxidizer_gui(dt: f64) {
            __oxidizer_with("gui", |m| $crate::Mod::gui(m, dt))
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn oxidizer_menu() {
            __oxidizer_with("menu", |m| $crate::Mod::menu(m))
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn oxidizer_loaded() {
            __oxidizer_with("loaded", |m| $crate::Mod::loaded(m))
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn oxidizer_unload() {
            __oxidizer_with("unload", |m| $crate::Mod::unload(m))
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn oxidizer_saving() {
            __oxidizer_with("saving", |m| $crate::Mod::saving(m))
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn oxidizer_save_loaded() {
            __oxidizer_with("save_loaded", |m| $crate::Mod::save_loaded(m))
        }
    };
}
