//! Mods talking to each other (Oxidizer 0.9 and later): an event bus and services. Values cross as JSON, so Rust
//! and Rhai mods work together; in Rust they are your own types, through serde.
//!
//! Names start with the owning mod's id: only `weather_core` may publish `weather_core.*` events or provide
//! `weather_core.*` services. Anyone may subscribe and call.
//!
//! ```ignore
//! use oxidizer_api::serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize)]
//! #[serde(crate = "oxidizer_api::serde", tag = "kind")]
//! pub enum Weather { Clear, Storm { strength: u8 } }
//! impl oxidizer_api::Event for Weather { const TOPIC: &'static str = "weather_core.weather"; }
//!
//! oxidizer_api::publish_retained(&Weather::Storm { strength: 3 })?;
//! let sub = oxidizer_api::subscribe(|w: Weather| { /* ... */ })?;   // keep `sub`; dropping it unsubscribes
//! ```

use crate::{api, log, read_string};
use serde::{Serialize, de::DeserializeOwned};

/// The mod's id (its folder name), as the loader gave it; empty on loaders before Oxidizer 0.9.
pub fn mod_id() -> &'static str {
    let a = api();
    if a.mod_id.is_null() {
        return "";
    }
    unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(a.mod_id, a.mod_id_len)) }
}

fn last_text() -> String {
    read_string(|b, c| (api().last_text)(b, c))
}

fn json<T: Serialize>(v: &T) -> Result<String, String> {
    serde_json::to_string(v).map_err(|e| e.to_string())
}

// ---- events ----

/// An event type: its topic, and its shape through serde. The topic starts with the publishing mod's id.
pub trait Event: Serialize + DeserializeOwned {
    const TOPIC: &'static str;
}

fn publish_raw(topic: &str, json: &str, retained: bool) -> Result<(), String> {
    let id = mod_id();
    let ok = (api().bus_publish)(id.as_ptr(), id.len(), topic.as_ptr(), topic.len(), json.as_ptr(), json.len(), retained as u8);
    if ok != 0 { Ok(()) } else { Err(last_text()) }
}

/// Publishes an event: subscribers get it at the start of the next frame, in order.
pub fn publish<E: Event>(event: &E) -> Result<(), String> {
    publish_raw(E::TOPIC, &json(event)?, false)
}

/// Publishes an event and keeps it as the topic's current value: later subscribers get it as soon as they subscribe.
pub fn publish_retained<E: Event>(event: &E) -> Result<(), String> {
    publish_raw(E::TOPIC, &json(event)?, true)
}

/// [`publish`] with a topic name and a JSON value.
pub fn publish_json(topic: &str, json: &str) -> Result<(), String> {
    publish_raw(topic, json, false)
}

/// [`publish_retained`] with a topic name and a JSON value.
pub fn publish_retained_json(topic: &str, json: &str) -> Result<(), String> {
    publish_raw(topic, json, true)
}

type Callback = Box<dyn Fn(&str) + Send + Sync>;

/// A subscription: dropping it unsubscribes. Keep it in your mod's state, or call [`Subscription::forever`].
pub struct Subscription {
    id: u64,
    callback: *mut Callback,
}

unsafe impl Send for Subscription {}

impl Subscription {
    /// Keeps the subscription for as long as the game runs.
    pub fn forever(self) {
        std::mem::forget(self);
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        (api().bus_unsubscribe)(self.id);
        drop(unsafe { Box::from_raw(self.callback) });
    }
}

extern "C" fn deliver(user: usize, json: *const u8, len: usize) {
    let f = unsafe { &*(user as *const Callback) };
    let text = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(json, len)) };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(text))).is_err() {
        log(&format!("{}: an event callback panicked", mod_id()));
    }
}

/// [`subscribe`] with a topic name, getting each value as JSON text.
pub fn subscribe_json(topic: &str, f: impl Fn(&str) + Send + Sync + 'static) -> Result<Subscription, String> {
    let callback: *mut Callback = Box::into_raw(Box::new(Box::new(f)));
    let id = mod_id();
    let sub = (api().bus_subscribe)(id.as_ptr(), id.len(), topic.as_ptr(), topic.len(), deliver, callback as usize);
    if sub == 0 {
        drop(unsafe { Box::from_raw(callback) });
        return Err(last_text());
    }
    Ok(Subscription { id: sub, callback })
}

/// Calls `f` with every event of type `E`, at the start of each frame (after it was published). A value that
/// doesn't match `E` is logged and skipped.
pub fn subscribe<E: Event + 'static>(f: impl Fn(E) + Send + Sync + 'static) -> Result<Subscription, String> {
    subscribe_json(E::TOPIC, move |text| match serde_json::from_str::<E>(text) {
        Ok(e) => f(e),
        Err(err) => log(&format!("{}: {} event doesn't fit its type: {err}", mod_id(), E::TOPIC)),
    })
}

// ---- services ----

/// A service: its name, and the shapes of its arguments and result. The name starts with the providing mod's id.
pub trait Service {
    const NAME: &'static str;
    type Args: Serialize + DeserializeOwned;
    type Out: Serialize + DeserializeOwned;
}

type Provider = Box<dyn Fn(&str) -> Result<String, String> + Send + Sync>;

extern "C" fn serve(user: usize, args: *const u8, len: usize) -> u8 {
    let f = unsafe { &*(user as *const Provider) };
    let text = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(args, len)) };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(text))).unwrap_or_else(|_| Err("the service panicked".into()));
    let (ok, reply) = match result {
        Ok(json) => (1, json),
        Err(e) => (0, e),
    };
    (api().service_reply)(reply.as_ptr(), reply.len());
    ok
}

/// [`provide`] with a service name, taking and returning JSON text.
pub fn provide_json(name: &str, f: impl Fn(&str) -> Result<String, String> + Send + Sync + 'static) -> Result<(), String> {
    let provider: *mut Provider = Box::into_raw(Box::new(Box::new(f)));
    let id = mod_id();
    if (api().service_provide)(id.as_ptr(), id.len(), name.as_ptr(), name.len(), serve, provider as usize) != 0 {
        Ok(()) // the provider stays registered for as long as the game runs
    } else {
        drop(unsafe { Box::from_raw(provider) });
        Err(last_text())
    }
}

/// Provides a service: other mods' calls run `f` right away, on their thread, and get its result.
pub fn provide<S: Service>(f: impl Fn(S::Args) -> Result<S::Out, String> + Send + Sync + 'static) -> Result<(), String> {
    provide_json(S::NAME, move |text| {
        let args = serde_json::from_str::<S::Args>(text).map_err(|e| format!("bad arguments: {e}"))?;
        json(&f(args)?)
    })
}

/// [`call`] with a service name and JSON arguments; returns the JSON result.
pub fn call_json(name: &str, args_json: &str) -> Result<String, String> {
    let id = mod_id();
    let ok = (api().service_call)(id.as_ptr(), id.len(), name.as_ptr(), name.len(), args_json.as_ptr(), args_json.len());
    let text = last_text();
    if ok != 0 { Ok(text) } else { Err(text) }
}

/// Calls a service and returns its result. Fails when nobody provides it, the provider is busy (it is running
/// further up this call), it fails, or the arguments or result don't fit.
pub fn call<S: Service>(args: &S::Args) -> Result<S::Out, String> {
    let out = call_json(S::NAME, &json(args)?)?;
    serde_json::from_str(&out).map_err(|e| format!("{} returned something that doesn't fit: {e}", S::NAME))
}
