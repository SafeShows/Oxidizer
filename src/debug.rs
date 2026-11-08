//! Breakpoints for Rhai mods. Rhai Studio sets and reads them through the Rhai functions `debug_breakpoints`,
//! `debug_state` and `debug_continue` (see script.rs), so any tool with the console can.
//!
//! Rhai calls its debugger on the game's thread, in the middle of a script: stopping there would freeze the game,
//! and the editor with it. So a breakpoint takes a snapshot instead (the variables, `this`, the call stack), lets
//! the call finish, and pauses that mod: Oxidizer stops calling it until `debug_continue`. The game keeps running.
//! A script's source name is "mod/file.rhai", which is how breakpoints name it.

use crate::log;
use rhai::debugger::{BreakPoint, DebuggerCommand, DebuggerEvent};
use rhai::{Dynamic, Engine, EvalContext, Position};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Mutex, MutexGuard};

/// Breakpoint lines by source.
static BREAKS: Mutex<BTreeMap<String, Vec<usize>>> = Mutex::new(BTreeMap::new());
/// The last snapshot at each breakpoint that was hit.
static HITS: Mutex<Vec<Hit>> = Mutex::new(Vec::new());
/// Mods paused at a breakpoint.
static PAUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Hit {
    mod_name: String,
    source: String,
    line: usize,
    count: u64,
    snapshot: Value,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Hooks the debugger into the engine. Every script run starts with the current breakpoints.
pub fn install(engine: &mut Engine) {
    #[allow(deprecated)] // Rhai flags this API as "volatile" with a deprecation, not as going away
    engine.register_debugger(
        |_, mut dbg| {
            for (source, lines) in lock(&BREAKS).iter() {
                for &line in lines {
                    let pos = Position::new(line.min(u16::MAX as usize) as u16, 0); // column 0: anything on the line
                    dbg.break_points_mut().push(BreakPoint::AtPosition { source: Some(source.into()), pos, enabled: true });
                }
            }
            dbg
        },
        |ctx, event, _node, source, pos| {
            if let (DebuggerEvent::BreakPoint(_), Some(source), Some(line)) = (event, source, pos.line()) {
                hit(&ctx, source, line);
            }
            Ok(DebuggerCommand::Continue)
        },
    );
}

fn text(v: &Dynamic) -> String {
    let s = v.to_string();
    if s.chars().count() > 200 { format!("{}...", s.chars().take(200).collect::<String>()) } else { s }
}

fn hit(ctx: &EvalContext, source: &str, line: usize) {
    let Some(mod_name) = source.split('/').next().map(str::to_string) else { return };
    if paused(&mod_name) {
        return; // already stopped in this call: the first breakpoint's snapshot stays
    }
    let vars: Vec<Value> = ctx.scope().iter().map(|(name, _, v)| json!([name, v.type_name(), text(&v)])).collect();
    let this: Vec<Value> = match ctx.this_ptr().and_then(|t| t.read_lock::<rhai::Map>().map(|m| m.clone())) {
        Some(map) => map.iter().map(|(k, v)| json!([k.as_str(), text(v)])).collect(),
        None => vec![],
    };
    let stack: Vec<Value> = ctx
        .global_runtime_state()
        .debugger()
        .call_stack()
        .iter()
        .rev()
        .map(|f| match f.pos.line() {
            Some(line) => json!(format!("{}() line {line}", f.fn_name)),
            None => json!(format!("{}()", f.fn_name)), // called by Oxidizer
        })
        .collect();
    let snapshot = json!({ "vars": vars, "this": this, "stack": stack });
    {
        let mut hits = lock(&HITS);
        match hits.iter_mut().find(|h| h.source == source && h.line == line) {
            Some(h) => {
                h.count += 1;
                h.snapshot = snapshot;
            }
            None => hits.push(Hit { mod_name: mod_name.clone(), source: source.into(), line, count: 1, snapshot }),
        }
    }
    lock(&PAUSED).push(mod_name.clone());
    log(&format!("{mod_name}: paused at {source} line {line}"));
}

/// Whether Oxidizer should leave this mod alone (it is paused at a breakpoint).
pub fn paused(mod_name: &str) -> bool {
    lock(&PAUSED).iter().any(|m| m == mod_name)
}

/// Sets the breakpoints of one source ("mod/file.rhai"); no lines clears them. Snapshots of removed breakpoints
/// go, and a mod paused at one runs on.
pub fn set_breakpoints(source: &str, lines: Vec<usize>) {
    let mut hits = lock(&HITS);
    hits.retain(|h| h.source != source || lines.contains(&h.line));
    if let Some(mod_name) = source.split('/').next()
        && !hits.iter().any(|h| h.mod_name == mod_name)
    {
        resume(mod_name);
    }
    if lines.is_empty() {
        lock(&BREAKS).remove(source);
    } else {
        lock(&BREAKS).insert(source.into(), lines);
    }
}

pub fn resume(mod_name: &str) {
    lock(&PAUSED).retain(|m| m != mod_name);
}

/// Everything as JSON: `{"breakpoints": {source: [lines]}, "hits": [{mod, source, line, count, paused, vars:
/// [[name, type, value]], this: [[key, value]], stack: ["fn() line n"]}]}`.
pub fn state() -> String {
    let paused: HashSet<String> = lock(&PAUSED).iter().cloned().collect();
    let hits: Vec<Value> = lock(&HITS)
        .iter()
        .map(|h| {
            let mut v = json!({ "mod": h.mod_name, "source": h.source, "line": h.line, "count": h.count, "paused": paused.contains(&h.mod_name) });
            if let (Some(o), Value::Object(s)) = (v.as_object_mut(), &h.snapshot) {
                o.extend(s.clone());
            }
            v
        })
        .collect();
    json!({ "breakpoints": *lock(&BREAKS), "hits": hits }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_breakpoint_takes_a_snapshot_and_pauses_the_mod() {
        let mut e = Engine::new();
        install(&mut e);
        let mut ast = e.compile("fn tick(n) {
    let x = n * 2;
    x + 1
}").unwrap();
        ast.set_source("dbg_test/main.rhai");
        set_breakpoints("dbg_test/main.rhai", vec![3]);
        let r: i64 = e.call_fn(&mut rhai::Scope::new(), &ast, "tick", (5_i64,)).unwrap();
        assert_eq!(r, 11); // the call itself finishes
        assert!(paused("dbg_test"));
        let s: Value = serde_json::from_str(&state()).unwrap();
        let hit = s["hits"].as_array().unwrap().iter().find(|h| h["mod"] == "dbg_test").unwrap();
        assert_eq!(hit["line"], 3);
        assert_eq!(hit["paused"], true);
        assert!(hit["vars"].as_array().unwrap().contains(&json!(["x", "i64", "10"])), "{}", hit["vars"]);
        resume("dbg_test");
        assert!(!paused("dbg_test"));
        set_breakpoints("dbg_test/main.rhai", vec![3]);
        e.call_fn::<i64>(&mut rhai::Scope::new(), &ast, "tick", (5_i64,)).unwrap();
        assert!(paused("dbg_test"));
        set_breakpoints("dbg_test/main.rhai", vec![]); // removing the breakpoint lets the mod run on
        assert!(!paused("dbg_test"));
        assert!(!state().contains("dbg_test"));
    }
}
