//! Physics forces example: while F7 is held, pushes the controlled vehicle along its body +X axis (the nose, for the
//! game's rockets) with twice its weight, so a rocket standing on the pad lifts off with no engine. Every 2 s it
//! logs how often the force callback ran and whether it saw the controlled vehicle.
//!
//! The callback runs on the game's physics worker threads: it reads only its input and atomics that `frame` sets.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

use oxidizer_api::physics::{self, ForceInput, ForceOutput};
use oxidizer_api::{Mod, Obj, key_down, log, oxidizer_mod};

static PUSH: AtomicBool = AtomicBool::new(false);
static TARGET: AtomicU64 = AtomicU64::new(0);
static CALLS: AtomicU64 = AtomicU64::new(0);
static TARGET_CALLS: AtomicU64 = AtomicU64::new(0);

const G0: f64 = 9.80665;

fn push(input: &ForceInput, out: &mut ForceOutput) -> bool {
    CALLS.fetch_add(1, Relaxed);
    if input.vehicle == 0 || input.vehicle != TARGET.load(Relaxed) {
        return false;
    }
    TARGET_CALLS.fetch_add(1, Relaxed);
    if !PUSH.load(Relaxed) {
        return false;
    }
    out.force = [2.0 * G0 * input.mass, 0.0, 0.0];
    true
}

struct ForceExample {
    since: f64,
}

impl Mod for ForceExample {
    fn init() -> Self {
        log(&format!("force_rust: register = {}", physics::register_force(push)));
        ForceExample { since: 0.0 }
    }

    fn frame(&mut self, _t: f64, dt: f64) {
        let vehicle = Obj::static_get("KSA.Program, KSA", "ControlledVehicle");
        TARGET.store(physics::vehicle_key(&vehicle), Relaxed);
        PUSH.store(key_down("F7"), Relaxed);
        self.since += dt;
        if self.since >= 2.0 {
            log(&format!(
                "force_rust: {} force calls, {} for the controlled vehicle (key {}), pushing: {}",
                CALLS.swap(0, Relaxed),
                TARGET_CALLS.swap(0, Relaxed),
                TARGET.load(Relaxed),
                PUSH.load(Relaxed)
            ));
            self.since = 0.0;
        }
    }
}

oxidizer_mod!(ForceExample);
