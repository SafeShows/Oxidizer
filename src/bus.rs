//! The event bus and services between mods (API version 5). Values are JSON, so Rust and Rhai mods work together.
//!
//! - A name belongs to the mod whose id starts it: only `weather_core` publishes `weather_core.*` events and
//!   provides `weather_core.*` services. Anyone subscribes and calls.
//! - Events are queued and delivered at the start of the next frame, in order: delivering while another script
//!   runs would have to skip busy scripts. A retained event is also kept as its topic's current value, which a new
//!   subscriber gets right away.
//! - Services run right away, on the caller's thread. A Rhai provider that is busy (running further up the same
//!   call) makes the call fail instead of waiting.

use crate::{log, script};
use oxidizer_api::{BusFn, ServiceFn};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

/// Queued events past this are dropped, oldest first, so a runaway publisher can't eat the game's memory.
const MAX_QUEUED: usize = 10_000;
/// Events kept for the Bus tab of the Oxidizer window.
const RECENT: usize = 30;

#[derive(Clone)]
pub enum Target {
    /// `owner`: the mod that subscribed, so a hot reload can take its subscriptions out.
    Rust { cb: BusFn, user: usize, owner: String },
    Rhai { script: String, function: String },
}

impl Target {
    /// The same Rhai subscriber (Rust subscribers are never merged).
    fn same_rhai(&self, other: &Target) -> bool {
        matches!((self, other), (Target::Rhai { script: a, function: f }, Target::Rhai { script: b, function: g }) if a == b && f == g)
    }
}

#[derive(Clone)]
struct Sub {
    id: u64,
    topic: String,
    target: Target,
}

#[derive(Clone)]
pub enum Provider {
    Rust { cb: ServiceFn, user: usize },
    Rhai { script: String, function: String },
}

struct Bus {
    subs: Vec<Sub>,
    /// (topic, JSON, only this subscription: a retained value for a new subscriber)
    queue: VecDeque<(String, String, Option<u64>)>,
    retained: BTreeMap<String, String>,
    recent: VecDeque<(String, String)>,
    /// name -> (owning mod, provider)
    services: BTreeMap<String, (String, Provider)>,
    next_id: u64,
    dropped: u64,
}

static BUS: Mutex<Bus> = Mutex::new(Bus {
    subs: Vec::new(),
    queue: VecDeque::new(),
    retained: BTreeMap::new(),
    recent: VecDeque::new(),
    services: BTreeMap::new(),
    next_id: 1,
    dropped: 0,
});

fn bus() -> std::sync::MutexGuard<'static, Bus> {
    BUS.lock().unwrap_or_else(|e| e.into_inner())
}

thread_local! {
    /// The last error of a bus or service call on this thread, or a service call's result: `last_text`.
    static LAST: RefCell<String> = const { RefCell::new(String::new()) };
    /// What the running Rust service callback replied.
    static REPLY: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Whether `caller` may use `name`: names start with the owner's mod id and a dot.
fn owns(caller: &str, name: &str) -> Result<(), String> {
    if caller.is_empty() {
        return Err(format!("{name}: this mod has no id (built for an older Oxidizer?)"));
    }
    if !name.starts_with(caller) || name.as_bytes().get(caller.len()) != Some(&b'.') || name.len() == caller.len() + 1 {
        return Err(format!("{caller} can only publish and provide names starting with \"{caller}.\", not \"{name}\""));
    }
    Ok(())
}

fn check_json(what: &str, json: &str) -> Result<(), String> {
    serde_json::from_str::<serde_json::Value>(json).map(|_| ()).map_err(|e| format!("{what} isn't JSON: {e}"))
}

pub fn publish(caller: &str, topic: &str, json: &str, retained: bool) -> Result<(), String> {
    owns(caller, topic)?;
    check_json(topic, json)?;
    let mut b = bus();
    if b.queue.len() >= MAX_QUEUED {
        b.queue.pop_front();
        b.dropped += 1;
        if b.dropped == 1 || b.dropped.is_multiple_of(1000) {
            log(&format!("event bus: more than {MAX_QUEUED} events waiting; dropped {} so far (last from {caller})", b.dropped));
        }
    }
    b.queue.push_back((topic.into(), json.into(), None));
    if retained {
        b.retained.insert(topic.into(), json.into());
    }
    if b.recent.len() == RECENT {
        b.recent.pop_front();
    }
    b.recent.push_back((topic.into(), json.into()));
    Ok(())
}

/// Subscribes; returns the subscription id. Subscribing a Rhai function to a topic twice keeps one subscription.
pub fn subscribe(topic: &str, target: Target) -> Result<u64, String> {
    if topic.is_empty() {
        return Err("subscribe: the topic is empty".into());
    }
    let mut b = bus();
    if let Some(s) = b.subs.iter().find(|s| s.topic == topic && s.target.same_rhai(&target)) {
        return Ok(s.id);
    }
    let id = b.next_id;
    b.next_id += 1;
    b.subs.push(Sub { id, topic: topic.into(), target });
    if let Some(value) = b.retained.get(topic).cloned() {
        b.queue.push_back((topic.into(), value, Some(id))); // the current value, for this subscriber only
    }
    Ok(id)
}

pub fn unsubscribe(id: u64) {
    bus().subs.retain(|s| s.id != id);
}

/// Takes out a Rust mod's subscriptions and services (before it is reloaded).
pub fn forget(owner: &str) {
    let mut b = bus();
    b.subs.retain(|s| !matches!(&s.target, Target::Rust { owner: o, .. } if o == owner));
    b.services.retain(|_, (o, p)| !(o == owner && matches!(p, Provider::Rust { .. })));
}

pub fn unsubscribe_target(topic: &str, target: &Target) {
    bus().subs.retain(|s| !(s.topic == topic && s.target.same_rhai(target)));
}

/// Delivers the events queued until now. Called at the start of each frame, before mods' frame().
pub fn deliver() {
    let (batch, subs) = {
        let mut b = bus();
        if b.queue.is_empty() {
            return;
        }
        (b.queue.drain(..).collect::<Vec<_>>(), b.subs.clone())
    };
    for (topic, json, only) in batch {
        for s in subs.iter().filter(|s| s.topic == topic && only.is_none_or(|o| o == s.id)) {
            match &s.target {
                Target::Rust { cb, user, owner } => {
                    // A callback may have unsubscribed another (and freed its data) during this delivery.
                    if bus().subs.iter().any(|x| x.id == s.id) {
                        crate::perf::time(owner, || cb(*user, json.as_ptr(), json.len()));
                    }
                }
                Target::Rhai { script, function } => script::deliver_event(script, function, &json),
            }
        }
    }
}

pub fn provide(caller: &str, name: &str, provider: Provider) -> Result<(), String> {
    owns(caller, name)?;
    let mut b = bus();
    if let Some((owner, _)) = b.services.get(name)
        && owner != caller
    {
        return Err(format!("{name} is already provided by {owner}"));
    }
    b.services.insert(name.into(), (caller.into(), provider)); // the same mod providing again replaces it
    Ok(())
}

/// Calls a service; the JSON result, or the error.
pub fn call(name: &str, args: &str) -> Result<String, String> {
    check_json("the arguments", args)?;
    let (owner, provider) = bus().services.get(name).cloned().ok_or_else(|| format!("no mod provides {name}"))?;
    match provider {
        Provider::Rust { cb, user } => {
            REPLY.with_borrow_mut(|r| *r = None);
            let ok = crate::perf::time(&owner, || cb(user, args.as_ptr(), args.len()));
            let reply = REPLY.with_borrow_mut(|r| r.take()).unwrap_or_default();
            if ok == 0 {
                return Err(format!("{name}: {reply}"));
            }
            check_json(&format!("{name}'s result"), &reply)?;
            Ok(reply)
        }
        Provider::Rhai { script, function } => script::call_service(&script, &function, args).map_err(|e| format!("{name}: {e}")),
    }
}

/// What the Bus tab shows: topics (subscribers, retained value), recent events, and services (names start with their owner).
pub struct Snapshot {
    pub topics: Vec<(String, usize, Option<String>)>,
    pub recent: Vec<(String, String)>,
    pub services: Vec<String>,
}

pub fn snapshot() -> Snapshot {
    let b = bus();
    let mut topics: BTreeMap<String, (usize, Option<String>)> = BTreeMap::new();
    for s in &b.subs {
        topics.entry(s.topic.clone()).or_default().0 += 1;
    }
    for (t, v) in &b.retained {
        topics.entry(t.clone()).or_default().1 = Some(v.clone());
    }
    Snapshot {
        topics: topics.into_iter().map(|(t, (n, v))| (t, n, v)).collect(),
        recent: b.recent.iter().rev().cloned().collect(),
        services: b.services.keys().cloned().collect(),
    }
}

// ---- the Api table's functions ----

fn text<'a>(p: *const u8, n: usize) -> &'a str {
    if p.is_null() {
        return "";
    }
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(p, n) }).unwrap_or("")
}

pub(crate) fn set_last(s: String) {
    LAST.with_borrow_mut(|l| *l = s);
}

pub extern "C" fn api_last_text(buf: *mut u8, cap: usize) -> usize {
    LAST.with_borrow(|l| crate::write_out(l, buf, cap))
}

pub extern "C" fn api_publish(c: *const u8, cn: usize, t: *const u8, tn: usize, j: *const u8, jn: usize, retained: u8) -> u8 {
    match publish(text(c, cn), text(t, tn), text(j, jn), retained != 0) {
        Ok(()) => 1,
        Err(e) => {
            set_last(e);
            0
        }
    }
}

pub extern "C" fn api_subscribe(c: *const u8, cn: usize, t: *const u8, tn: usize, cb: BusFn, user: usize) -> u64 {
    subscribe(text(t, tn), Target::Rust { cb, user, owner: text(c, cn).into() }).unwrap_or_else(|e| {
        set_last(e);
        0
    })
}

pub extern "C" fn api_unsubscribe(id: u64) {
    unsubscribe(id);
}

pub extern "C" fn api_provide(c: *const u8, cn: usize, n: *const u8, nn: usize, cb: ServiceFn, user: usize) -> u8 {
    match provide(text(c, cn), text(n, nn), Provider::Rust { cb, user }) {
        Ok(()) => 1,
        Err(e) => {
            set_last(e);
            0
        }
    }
}

pub extern "C" fn api_reply(p: *const u8, n: usize) {
    let s = text(p, n).to_string();
    REPLY.with_borrow_mut(|r| *r = Some(s));
}

pub extern "C" fn api_call(_c: *const u8, _cn: usize, n: *const u8, nn: usize, a: *const u8, an: usize) -> u8 {
    let (ok, s) = match call(text(n, nn), text(a, an)) {
        Ok(r) => (1, r),
        Err(e) => (0, e),
    };
    set_last(s);
    ok
}

#[cfg(test)]
mod tests {
    use super::owns;

    #[test]
    fn names_belong_to_their_mod() {
        assert!(owns("weather", "weather.changed").is_ok());
        assert!(owns("weather", "weather_core.changed").is_err());
        assert!(owns("weather", "weatherx").is_err());
        assert!(owns("weather", "weather.").is_err());
        assert!(owns("", "weather.changed").is_err());
        assert!(owns("weather", "other.changed").is_err());
    }

    #[test]
    fn forget_takes_out_a_rust_mods_subscriptions_and_services() {
        use super::*;
        extern "C" fn on_event(_: usize, _: *const u8, _: usize) {}
        extern "C" fn serve(_: usize, _: *const u8, _: usize) -> u8 {
            1
        }
        let mine = Target::Rust { cb: on_event, user: 0, owner: "hot_a".into() };
        let other = Target::Rust { cb: on_event, user: 0, owner: "hot_b".into() };
        subscribe("hot_test.topic", mine).unwrap();
        subscribe("hot_test.topic", other).unwrap();
        provide("hot_a", "hot_a.svc", Provider::Rust { cb: serve, user: 0 }).unwrap();
        forget("hot_a");
        let b = bus();
        let owners: Vec<&str> = b.subs.iter().filter_map(|s| match &s.target {
            Target::Rust { owner, .. } if s.topic == "hot_test.topic" => Some(owner.as_str()),
            _ => None,
        }).collect();
        assert_eq!(owners, ["hot_b"]);
        assert!(!b.services.contains_key("hot_a.svc"));
    }
}
