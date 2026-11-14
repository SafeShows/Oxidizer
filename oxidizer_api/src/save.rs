//! Data kept per save game (Oxidizer 0.12 and later): values a mod sets here are written with the player's save and
//! come back when that save is loaded. They live in the save's folder as `oxidizer/<mod>.json`.
//!
//! ```ignore
//! fn saving(&mut self) {
//!     oxidizer_api::save::set("missions_done", &self.missions_done);   // the game is about to write a save
//! }
//! fn save_loaded(&mut self) {
//!     self.missions_done = oxidizer_api::save::get("missions_done").unwrap_or_default();
//! }
//! ```
//!
//! Values are anything serde can turn into JSON. A game started without loading a save has none.

use crate::{api, read_string};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// This mod's value for `key` in the current save, as JSON text.
pub fn get_json(key: &str) -> Option<String> {
    let id = crate::bus::mod_id();
    ((api().save_get)(id.as_ptr(), id.len(), key.as_ptr(), key.len()) != 0).then(|| read_string(|b, c| (api().last_text)(b, c)))
}

/// This mod's value for `key`, or None if there isn't one (or it doesn't fit `T`).
pub fn get<T: DeserializeOwned>(key: &str) -> Option<T> {
    serde_json::from_str(&get_json(key)?).ok()
}

/// Sets this mod's `key` to a JSON value (`null` removes it). False if `json` isn't valid JSON.
pub fn set_json(key: &str, json: &str) -> bool {
    let id = crate::bus::mod_id();
    (api().save_set)(id.as_ptr(), id.len(), key.as_ptr(), key.len(), json.as_ptr(), json.len()) != 0
}

/// Sets this mod's `key`; it is written with the next save.
pub fn set<T: Serialize + ?Sized>(key: &str, value: &T) {
    if let Ok(json) = serde_json::to_string(value) {
        set_json(key, &json);
    }
}

/// Removes this mod's `key`.
pub fn remove(key: &str) {
    set_json(key, "null");
}
