//! Talks with a C# StarMap mod (examples/starmap_cs) through StarMap's event bus. Needs Oxidizer.StarMap.
//! The C# records `Ping(int Count)` and `Pong(string From, int Count)` are mirrored as serde structs.

use oxidizer_api::serde::{Deserialize, Serialize};
use oxidizer_api::{Mod, log, starmap};

#[derive(Deserialize)]
#[serde(crate = "oxidizer_api::serde")]
struct Ping {
    #[serde(rename = "Count")]
    count: i64,
}

#[derive(Serialize)]
#[serde(crate = "oxidizer_api::serde")]
struct Pong<'a> {
    #[serde(rename = "From")]
    from: &'a str,
    #[serde(rename = "Count")]
    count: i64,
}

struct StarMapRust {
    /// Kept while the mod runs: dropping it unsubscribes.
    _pings: Option<starmap::Subscription>,
}

impl Mod for StarMapRust {
    fn init() -> Self {
        let pings = starmap::subscribe_as("StarTest.Ping, StarTest", "", |p: Ping| {
            log(&format!("starmap_rust: got ping {}", p.count));
            if let Err(e) = starmap::publish("StarTest.Pong, StarTest", "", &Pong { from: "Rust", count: p.count }) {
                log(&format!("starmap_rust: {e}"));
            }
        });
        if let Err(e) = &pings {
            log(&format!("starmap_rust: {e}"));
        }
        StarMapRust { _pings: pings.ok() }
    }
}

oxidizer_api::oxidizer_mod!(StarMapRust);
