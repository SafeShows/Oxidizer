//! StarMap's event bus, for Oxidizer's mods (when Oxidizer runs as the StarMap mod Oxidizer.StarMap, StarMap 0.5.2+).
//! StarMap's messages are C# objects matched by type, so a mod names the type ("Namespace.Type, Assembly") and
//! optionally a channel (StarMap's named channels; "" is the default one).
//!
//! - subscribe: a Rust callback or a Rhai function gets each message as a game object (Obj), and its JSON.
//! - publish: a game object of the type, or JSON that the bridge turns into one (System.Text.Json).
//! - forward: copies a type's messages onto Oxidizer's own bus as JSON, topic `starmap.<Type>[/<channel>]`, so
//!   `subscribe(topic, ...)` code works unchanged.
//!
//! Messages arrive at the start of the next frame, on the game's thread (StarMap delivers on the publisher's
//! thread; the bridge queues them, see bridge/StarMapEvents.cs).

use crate::{bus, internal, log};
use oxidizer_api::{Handle, Obj, read_string};
use std::sync::{Mutex, MutexGuard};

pub type StarMapFn = oxidizer_api::starmap::StarMapFn;

#[derive(Clone)]
pub enum Target {
    Rust { cb: StarMapFn, user: usize },
    Rhai { script: String, function: String },
    Forward { topic: String },
}

#[derive(Clone)]
struct Sub {
    /// The bridge's subscription id.
    id: i64,
    owner: String,
    ty: String,
    channel: String,
    target: Target,
}

static SUBS: Mutex<Vec<Sub>> = Mutex::new(Vec::new());

fn subs() -> MutexGuard<'static, Vec<Sub>> {
    SUBS.lock().unwrap_or_else(|e| e.into_inner())
}

const NOT_HERE: &str = "StarMap's event bus is only there when Oxidizer runs as a StarMap mod (Oxidizer.StarMap, StarMap 0.5.2 or later)";

fn last_error() -> String {
    internal().starmap_error.map(|f| read_string(|b, c| f(b, c))).unwrap_or_else(|| NOT_HERE.into())
}

/// Whether StarMap's bus is there.
pub fn available() -> bool {
    internal().starmap_available.is_some_and(|f| f() != 0)
}

/// `starmap.<full type name>` (without the assembly), plus `/<channel>` for a named channel.
pub fn topic(ty: &str, channel: &str) -> String {
    let name = ty.split(',').next().unwrap_or(ty).trim();
    if channel.is_empty() { format!("starmap.{name}") } else { format!("starmap.{name}/{channel}") }
}

fn same(a: &Target, b: &Target) -> bool {
    match (a, b) {
        (Target::Rhai { script: s, function: f }, Target::Rhai { script: t, function: g }) => s == t && f == g,
        (Target::Forward { .. }, Target::Forward { .. }) => true,
        _ => false, // Rust callbacks are never merged
    }
}

/// Subscribes; returns the subscription's id. A Rhai function, or a forward, is subscribed once per type and channel.
pub fn subscribe(owner: &str, ty: &str, channel: &str, target: Target) -> Result<i64, String> {
    if let Some(s) = subs().iter().find(|s| s.ty == ty && s.channel == channel && same(&s.target, &target)) {
        return Ok(s.id);
    }
    let f = internal().starmap_subscribe.ok_or(NOT_HERE)?;
    let id = f(ty.as_ptr(), ty.len(), channel.as_ptr(), channel.len());
    if id == 0 {
        return Err(last_error());
    }
    subs().push(Sub { id, owner: owner.into(), ty: ty.into(), channel: channel.into(), target });
    Ok(id)
}

fn drop_subs(keep: impl Fn(&Sub) -> bool) {
    let gone: Vec<i64> = {
        let mut all = subs();
        let gone = all.iter().filter(|s| !keep(s)).map(|s| s.id).collect();
        all.retain(|s| keep(s));
        gone
    };
    if let Some(f) = internal().starmap_unsubscribe {
        gone.into_iter().for_each(|id| f(id));
    }
}

pub fn unsubscribe(id: i64) {
    drop_subs(|s| s.id != id);
}

pub fn unsubscribe_target(ty: &str, channel: &str, target: &Target) {
    drop_subs(|s| !(s.ty == ty && s.channel == channel && same(&s.target, target)));
}

/// Takes out a Rust mod's subscriptions (before a hot reload).
pub fn forget(owner: &str) {
    drop_subs(|s| !(s.owner == owner && matches!(s.target, Target::Rust { .. })));
}

/// Publishes `handle` (a game object of the type), or when it is 0, the JSON made into one.
pub fn publish(ty: &str, channel: &str, json: &str, handle: Handle) -> Result<(), String> {
    let f = internal().starmap_publish.ok_or(NOT_HERE)?;
    if f(ty.as_ptr(), ty.len(), channel.as_ptr(), channel.len(), json.as_ptr(), json.len(), handle) == 0 {
        return Err(last_error());
    }
    Ok(())
}

/// Starts copying a type's messages onto Oxidizer's bus; returns the topic.
pub fn forward(ty: &str, channel: &str) -> Result<String, String> {
    let t = topic(ty, channel);
    subscribe("starmap", ty, channel, Target::Forward { topic: t.clone() })?;
    Ok(t)
}

/// Once the game has loaded (every StarMap mod is in by then): says which subscribed types never appeared.
pub fn report_waiting() {
    let Some(f) = internal().starmap_waiting else { return };
    for ty in read_string(|b, c| f(b, c)).lines().filter(|l| !l.is_empty()) {
        log(&format!("StarMap: no mod has the type {ty}; subscriptions to it wait for it (is the mod installed and enabled?)"));
    }
}

/// Hands out the messages StarMap delivered since the last frame. Called at the start of each frame.
pub fn deliver() {
    let (Some(next), Some(json_of)) = (internal().starmap_next, internal().starmap_json) else { return };
    let (mut id, mut handle) = (0i64, 0 as Handle);
    while next(&mut id, &mut handle) != 0 {
        let json = read_string(|b, c| json_of(b, c));
        let sub = subs().iter().find(|s| s.id == id).cloned();
        let obj = unsafe { Obj::from_handle(handle) }; // freed when it goes out of scope, unless handed on
        let Some(sub) = sub else { continue }; // unsubscribed meanwhile
        match sub.target {
            Target::Rust { cb, user } => {
                let h = obj.handle();
                std::mem::forget(obj); // the callback owns it now
                crate::perf::time(&sub.owner, || cb(user, h, json.as_ptr(), json.len()));
            }
            Target::Rhai { script, function } => crate::script::deliver_obj(&script, &function, obj),
            Target::Forward { topic } => {
                if let Err(e) = bus::publish("starmap", &topic, &json, false) {
                    log(&format!("forwarding {} from StarMap: {e}", sub.ty));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn topics() {
        assert_eq!(super::topic("StarTest.Ping, StarTest", ""), "starmap.StarTest.Ping");
        assert_eq!(super::topic("StarTest.Ping", "panel"), "starmap.StarTest.Ping/panel");
    }
}
