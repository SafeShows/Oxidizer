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

pub mod imgui;

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
}

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

/// Implement and register with [`oxidizer_mod!`].
pub trait Mod: Send + 'static {
    fn init() -> Self;
    /// After `KSA.Program.OnFrame`.
    fn frame(&mut self, _player_time: f64, _dt: f64) {}
    /// Inside the ImGui frame; draw windows here with [`imgui`].
    fn gui(&mut self, _dt: f64) {}
    fn unload(&mut self) {}
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
        fn __ksa_with(name: &str, f: impl FnOnce(&mut $t)) {
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
            __ksa_with("frame", |m| $crate::Mod::frame(m, t, dt))
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn oxidizer_gui(dt: f64) {
            __ksa_with("gui", |m| $crate::Mod::gui(m, dt))
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn oxidizer_unload() {
            __ksa_with("unload", |m| $crate::Mod::unload(m))
        }
    };
}
