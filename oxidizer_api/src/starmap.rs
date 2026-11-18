//! StarMap's event bus (Oxidizer 0.13 and later, running as the StarMap mod Oxidizer.StarMap with StarMap 0.5.2+):
//! talk with StarMap's C# mods. StarMap matches messages by their C# type, so they are named by type,
//! `"Namespace.Type, Assembly"`, plus a channel (StarMap's named channels; `""` is the default one).
//!
//! ```ignore
//! use oxidizer_api::starmap;
//!
//! // Their message as a typed struct (fields match the C# properties, case doesn't matter):
//! #[derive(serde::Deserialize, serde::Serialize)]
//! #[serde(crate = "oxidizer_api::serde")]
//! struct Ping { count: i64 }
//!
//! let sub = starmap::subscribe_as("StarTest.Ping, StarTest", "", |p: Ping| log(&format!("ping {}", p.count)))?;
//! starmap::publish("StarTest.Pong, StarTest", "", &Ping { count: 1 })?;
//! ```
//!
//! Messages arrive at the start of the next frame, on the game's thread. Without StarMap, every function returns
//! an error saying so.

use crate::{Handle, Obj, api, log, read_string};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// (user, the message as a game object handle (the callee owns it), its JSON, JSON length)
pub type StarMapFn = extern "C" fn(user: usize, message: Handle, json: *const u8, len: usize);

type Callback = Box<dyn Fn(Obj, &str) + Send + Sync>;

fn last_text() -> String {
    read_string(|b, c| (api().last_text)(b, c))
}

/// A subscription to a StarMap message type: dropping it unsubscribes. Keep it in your mod's state, or call
/// [`Subscription::forever`].
pub struct Subscription {
    id: u64,
    callback: *mut Callback,
}

unsafe impl Send for Subscription {}

impl Subscription {
    pub fn forever(self) {
        std::mem::forget(self);
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        (api().starmap_unsubscribe)(self.id);
        drop(unsafe { Box::from_raw(self.callback) });
    }
}

extern "C" fn deliver(user: usize, message: Handle, json: *const u8, len: usize) {
    let f = unsafe { &*(user as *const Callback) };
    let obj = unsafe { Obj::from_handle(message) };
    let text = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(json, len)) };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(obj, text))).is_err() {
        log(&format!("{}: a StarMap event callback panicked", crate::bus::mod_id()));
    }
}

/// Calls `f` with each message of a C# type (and channel), as the game object and as JSON.
pub fn subscribe(type_name: &str, channel: &str, f: impl Fn(Obj, &str) + Send + Sync + 'static) -> Result<Subscription, String> {
    let callback: *mut Callback = Box::into_raw(Box::new(Box::new(f)));
    let me = crate::bus::mod_id();
    let id = (api().starmap_subscribe)(me.as_ptr(), me.len(), type_name.as_ptr(), type_name.len(), channel.as_ptr(), channel.len(), deliver, callback as usize);
    if id == 0 {
        drop(unsafe { Box::from_raw(callback) });
        return Err(last_text());
    }
    Ok(Subscription { id, callback })
}

/// [`subscribe`], with each message turned into `T` from its JSON. Messages that don't fit are logged and skipped.
pub fn subscribe_as<T: DeserializeOwned + 'static>(type_name: &str, channel: &str, f: impl Fn(T) + Send + Sync + 'static) -> Result<Subscription, String> {
    let name = type_name.to_string();
    subscribe(type_name, channel, move |_, json| match serde_json::from_str::<T>(json) {
        Ok(v) => f(v),
        Err(e) => log(&format!("{}: a {name} message doesn't fit its type: {e}", crate::bus::mod_id())),
    })
}

fn publish_raw(type_name: &str, channel: &str, json: &str, handle: Handle) -> Result<(), String> {
    if (api().starmap_publish)(type_name.as_ptr(), type_name.len(), channel.as_ptr(), channel.len(), json.as_ptr(), json.len(), handle) == 0 {
        return Err(last_text());
    }
    Ok(())
}

/// Publishes a message of a C# type made from `value`'s JSON (property names match, case doesn't matter).
pub fn publish<T: Serialize + ?Sized>(type_name: &str, channel: &str, value: &T) -> Result<(), String> {
    publish_raw(type_name, channel, &serde_json::to_string(value).map_err(|e| e.to_string())?, 0)
}

/// [`publish`] with JSON text.
pub fn publish_json(type_name: &str, channel: &str, json: &str) -> Result<(), String> {
    publish_raw(type_name, channel, json, 0)
}

/// Publishes a game object that already is of the type.
pub fn publish_obj(type_name: &str, channel: &str, message: &Obj) -> Result<(), String> {
    publish_raw(type_name, channel, "", message.handle())
}

/// Copies a type's messages onto Oxidizer's own event bus as JSON, and returns the topic,
/// `starmap.<Namespace.Type>` (plus `/<channel>`): [`crate::subscribe_json`] on it works like for any event.
pub fn forward(type_name: &str, channel: &str) -> Result<String, String> {
    if (api().starmap_forward)(type_name.as_ptr(), type_name.len(), channel.as_ptr(), channel.len()) == 0 {
        return Err(last_text());
    }
    Ok(last_text())
}
