//! bus_rust: the Rust half of the event bus example; bus_rhai is the Rhai half. Events and services are typed:
//! the types say the shape, and their JSON is what crosses to the other mod (any language).

use oxidizer_api::serde::{Deserialize, Serialize};
use oxidizer_api::{Event, Mod, Service, Subscription, log};

// bus_rhai's weather: { "kind": "Storm", "strength": 3 } or { "kind": "Clear" }
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "oxidizer_api::serde", tag = "kind")]
enum Weather {
    Clear,
    Storm { strength: i64 },
}
impl Event for Weather {
    const TOPIC: &'static str = "bus_rhai.weather";
}

// This mod's own event: names start with this mod's id.
#[derive(Serialize, Deserialize)]
#[serde(crate = "oxidizer_api::serde")]
struct Tick {
    frame: u64,
}
impl Event for Tick {
    const TOPIC: &'static str = "bus_rust.tick";
}

// A service this mod provides: { "a": 2, "b": 3 } -> 5
#[derive(Serialize, Deserialize)]
#[serde(crate = "oxidizer_api::serde")]
struct AddArgs {
    a: f64,
    b: f64,
}
struct Add;
impl Service for Add {
    const NAME: &'static str = "bus_rust.add";
    type Args = AddArgs;
    type Out = f64;
}

// A service bus_rhai provides.
#[derive(Serialize, Deserialize)]
#[serde(crate = "oxidizer_api::serde")]
struct GreetArgs {
    name: String,
}
#[derive(Serialize, Deserialize)]
#[serde(crate = "oxidizer_api::serde")]
struct Greeting {
    text: String,
}
struct Greet;
impl Service for Greet {
    const NAME: &'static str = "bus_rhai.greet";
    type Args = GreetArgs;
    type Out = Greeting;
}

struct BusRust {
    frames: u64,
    /// Kept for as long as the mod runs: dropping it would unsubscribe.
    _weather: Option<Subscription>,
}

impl Mod for BusRust {
    fn init() -> Self {
        if let Err(e) = oxidizer_api::provide::<Add>(|args| Ok(args.a + args.b)) {
            log(&format!("bus_rust: {e}"));
        }
        let weather = oxidizer_api::subscribe(|w: Weather| log(&format!("bus_rust: the weather is {w:?}")))
            .map_err(|e| log(&format!("bus_rust: {e}")))
            .ok();
        // Names belong to their mod: this one is bus_rhai's, so it is refused.
        if let Err(e) = oxidizer_api::publish_json("bus_rhai.weather", r#"{"kind":"Clear"}"#) {
            log(&format!("bus_rust: refused, as it should be: {e}"));
        }
        BusRust { frames: 0, _weather: weather }
    }

    fn frame(&mut self, _t: f64, _dt: f64) {
        self.frames += 1;
        if self.frames == 60 {
            match oxidizer_api::call::<Greet>(&GreetArgs { name: "Rust".into() }) {
                Ok(g) => log(&format!("bus_rust: bus_rhai says \"{}\"", g.text)),
                Err(e) => log(&format!("bus_rust: greet failed: {e}")),
            }
        }
        if self.frames.is_multiple_of(300) {
            let _ = oxidizer_api::publish(&Tick { frame: self.frames });
        }
    }
}

oxidizer_api::oxidizer_mod!(BusRust);
