//! Forces on vehicles from a mod (API version 6): aerodynamics, wind, magnetic torquers, anything the game's physics
//! doesn't have.
//!
//! A force callback runs inside the game's physics, once or more per substep for every vehicle in full physics
//! (vehicles on rails and landed ones have no substeps). It runs on the game's worker threads, several vehicles at
//! once, and the same vehicle's substeps move between threads. So the callback:
//! - gets everything it needs in [`ForceInput`] and must not call into the game (no [`Obj`], no hooks, no logging);
//! - must be reentrant: keep per-vehicle state keyed by [`ForceInput::vehicle`] (see [`vehicle_key`]), not per thread;
//! - should be fast: it runs inside the game's physics step.
//!
//! What it returns is added to the vehicle's forces, after the game's own (gravity, drag, engines, parachutes).
//!
//! ```ignore
//! use oxidizer_api::physics::{self, ForceInput, ForceOutput};
//!
//! fn push(input: &ForceInput, out: &mut ForceOutput) -> bool {
//!     out.force = [0.0, 0.0, 100.0]; // 100 N along the vehicle's +Z axis
//!     true
//! }
//! physics::register_force(push); // in init
//! ```
//!
//! [`Obj`]: crate::Obj

use crate::{Obj, api};

/// A vehicle's physics state at one evaluation. Must match `ForceInput` in the bridge (`bridge/Forces.cs`).
///
/// "Bub" is the game's physics frame: centered on the parent body, with axes either inertial (`frame` 0, CCI) or
/// rotating with the body (`frame` 1, CCF, used near bodies). "Body" axes are the vehicle's, the same axes as its
/// assembly (`KSA.Part.PositionVehicleAsmb`). Units are SI.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ForceInput {
    /// Oxidizer's key for the vehicle: the same as [`vehicle_key`] for its `KSA.Vehicle`. 0 if unknown.
    pub vehicle: u64,
    /// 0: CCI (inertial axes), 1: CCF (axes rotating with the parent body).
    pub frame: i32,
    /// 1 when the vehicle is close enough to its parent body for the game to apply drag and buoyancy.
    pub in_physics_radius: i32,
    /// Substep length, s.
    pub dt: f64,
    /// Time into the step, s.
    pub intra_step_time: f64,
    /// Vehicle center of mass relative to the parent body, Bub, m.
    pub position: [f64; 3],
    /// Bub, m/s. In CCF this is already the velocity relative to the (rigidly rotating) air.
    pub velocity: [f64; 3],
    /// Rotation from body to Bub axes, quaternion (x, y, z, w).
    pub body_to_bub: [f64; 4],
    /// Bub, rad/s.
    pub angular_velocity: [f64; 3],
    /// The parent body's angular velocity (CCI axes), rad/s.
    pub parent_spin: [f64; 3],
    /// Atmosphere at the vehicle, from the game's model (all 0 above the atmosphere): kg/m³, Pa, K, m/s.
    pub density: f64,
    pub pressure: f64,
    pub temperature: f64,
    pub speed_of_sound: f64,
    /// Liquid the vehicle is in, if any: kg/m³, immersed m³ and m².
    pub ocean_density: f64,
    pub ocean_volume: f64,
    pub ocean_surface_area: f64,
    /// kg.
    pub mass: f64,
    /// Assembly frame, m.
    pub center_of_mass: [f64; 3],
    /// About the center of mass, m.
    pub bounding_sphere_radius: f64,
    /// The game's estimates, m² and m³.
    pub surface_area: f64,
    pub volume: f64,
    /// The game's own drag areas (Cd·A, m²) per body axis, for flow along +axis and along −axis.
    pub stock_cda_positive: [f64; 3],
    pub stock_cda_negative: [f64; 3],
}

/// What a force callback adds: body axes, about the center of mass.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ForceOutput {
    /// N.
    pub force: [f64; 3],
    /// N·m.
    pub torque: [f64; 3],
}

/// Raw force callback: (user data, input, output) -> 1 when `output` holds something to add.
pub type ForceFn = extern "C" fn(user: usize, input: *const ForceInput, output: *mut ForceOutput) -> u8;

/// Registers a raw force callback (see [`ForceFn`]). A mod can register several; all are summed. They are removed
/// when the mod is reloaded. False when the game's physics can't be patched (the log says why).
pub fn register_force_raw(cb: ForceFn, user: usize) -> bool {
    (api().force_register)(cb, user) != 0
}

/// Registers `f` as a force callback: it fills the output and returns true when there is something to add.
/// A panic in `f` is caught and counts as no force.
pub fn register_force(f: fn(&ForceInput, &mut ForceOutput) -> bool) -> bool {
    extern "C" fn tramp(user: usize, input: *const ForceInput, output: *mut ForceOutput) -> u8 {
        let f: fn(&ForceInput, &mut ForceOutput) -> bool = unsafe { std::mem::transmute(user) };
        let (input, output) = unsafe { (&*input, &mut *output) };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(input, output))).unwrap_or(false) as u8
    }
    register_force_raw(tramp, f as usize)
}

/// The key force callbacks get in [`ForceInput::vehicle`] for this `KSA.Vehicle` (0 for null). Stable for the
/// vehicle's lifetime; call it on the game's thread (in `frame`, `loaded`, a hook).
pub fn vehicle_key(vehicle: &Obj) -> u64 {
    (api().vehicle_key)(vehicle.handle())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    /// The bridge's C# structs (bridge/Forces.cs) have the same layout; its offline test checks the same numbers.
    #[test]
    fn layout_matches_the_bridge() {
        assert_eq!(size_of::<ForceInput>(), 320);
        assert_eq!(offset_of!(ForceInput, dt), 16);
        assert_eq!(offset_of!(ForceInput, position), 32);
        assert_eq!(offset_of!(ForceInput, density), 160);
        assert_eq!(offset_of!(ForceInput, mass), 216);
        assert_eq!(offset_of!(ForceInput, stock_cda_negative), 296);
        assert_eq!(size_of::<ForceOutput>(), 48);
    }
}
