//! Mods' physics forces (API version 6). The bridge calls [`dispatch`] from inside the game's physics
//! (`KSA.PhysicsStates.ComputeDerivatives`, see bridge/Forces.cs) on its vehicle worker threads, several at once;
//! it sums what the registered callbacks return. The game is only patched once the first callback registers.

use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

use oxidizer_api::physics::{ForceFn, ForceInput, ForceOutput};

struct Force {
    id: &'static str,
    cb: ForceFn,
    user: usize,
    /// Time spent in the callback since the last frame, for the Performance view (perf's own timing locks).
    nanos: AtomicU64,
}

// ponytail: registrations are leaked (a few bytes each, only on register/reload) so callers on worker threads can
// hold plain references; free them if mods ever register in a loop.
static FORCES: RwLock<Vec<&'static Force>> = RwLock::new(Vec::new());
static ENABLED: AtomicBool = AtomicBool::new(false);

fn read() -> std::sync::RwLockReadGuard<'static, Vec<&'static Force>> {
    FORCES.read().unwrap_or_else(|e| e.into_inner())
}

/// Registers a callback for mod `id`, patching the game the first time.
pub fn register(id: &'static str, cb: ForceFn, user: usize) -> u8 {
    if !ENABLED.load(Relaxed) {
        let Some(enable) = crate::internal().enable_forces else {
            crate::log("physics forces need a newer bridge (bridge/Oxidizer.Bridge.dll is older than Oxidizer)");
            return 0;
        };
        if enable() == 0 {
            return 0; // the bridge logged why
        }
        ENABLED.store(true, Relaxed);
    }
    let force: &'static Force = Box::leak(Box::new(Force { id, cb, user, nanos: AtomicU64::new(0) }));
    FORCES.write().unwrap_or_else(|e| e.into_inner()).push(force);
    1
}

/// Takes out a mod's callbacks (it is being reloaded). Waits for calls in progress, which hold the read lock.
pub fn forget(id: &str) {
    FORCES.write().unwrap_or_else(|e| e.into_inner()).retain(|f| f.id != id);
}

/// The bridge's entry: sums every callback's force into `output`; 1 when there is any.
pub extern "C" fn dispatch(input: *const ForceInput, output: *mut ForceOutput) -> u8 {
    let forces = read();
    let mut sum = ForceOutput::default();
    let mut any = false;
    for f in forces.iter() {
        let mut out = ForceOutput::default();
        let start = Instant::now();
        let added = (f.cb)(f.user, input, &mut out) != 0;
        f.nanos.fetch_add(start.elapsed().as_nanos() as u64, Relaxed);
        if added {
            any = true;
            for i in 0..3 {
                sum.force[i] += out.force[i];
                sum.torque[i] += out.torque[i];
            }
        }
    }
    if any {
        unsafe { *output = sum };
    }
    any as u8
}

/// Once per frame, on the game's thread: moves the callbacks' time into the Performance view.
pub fn report_time() {
    for f in read().iter() {
        let n = f.nanos.swap(0, Relaxed);
        if n > 0 {
            crate::perf::add(f.id, Duration::from_nanos(n));
        }
    }
}

/// The loader's own table: callbacks registered outside any mod's callbacks.
pub extern "C" fn api_register(cb: ForceFn, user: usize) -> u8 {
    register("(unknown)", cb, user)
}

pub extern "C" fn api_vehicle_key(vehicle: oxidizer_api::Handle) -> u64 {
    crate::internal().vehicle_key.map_or(0, |key| key(vehicle))
}
