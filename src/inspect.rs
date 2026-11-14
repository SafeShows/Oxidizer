//! `inspect(value)`: what a game object holds right now, as JSON, for Rhai Studio's inspector (and any tool with
//! the console). Reflection on the live object: its public properties and fields with their types and current
//! values, and the first items of a collection.

use crate::internal;
use oxidizer_api::Obj;
use serde_json::{Value, json};

const MAX_ITEMS: usize = 100;
const MAX_TEXT: usize = 160;

/// "null", "bool", "int", "float", "string" or "object".
fn kind(o: &Obj) -> &'static str {
    match (internal().kind)(o.handle()) {
        0 => "null",
        1 => "bool",
        2 => "int",
        3 => "float",
        4 => "string",
        _ => "object",
    }
}

fn text(o: &Obj) -> String {
    let s = o.to_string();
    if s.chars().count() > MAX_TEXT { format!("{}...", s.chars().take(MAX_TEXT).collect::<String>()) } else { s }
}

fn truthy(o: &Obj) -> bool {
    o.f64().is_some_and(|v| v != 0.0)
}

/// `{"type", "text", "count", "members": [{"name", "member", "type", "kind", "value", "static", "writable"}],
/// "items": [{"index", "kind", "value"}]}`.
pub fn inspect(o: &Obj) -> Value {
    if o.is_null() {
        return json!({ "type": "null", "text": "null", "count": null, "members": [], "items": [] });
    }
    let t = o.call("GetType", &[]);
    let mut members = vec![];
    let props = t.call("GetProperties", &[]);
    for i in 0..props.len().unwrap_or(0) {
        let p = props.index(i);
        let getter = p.call("GetGetMethod", &[]); // null when the getter isn't public
        if getter.is_null() || props_indexed(&p) {
            continue;
        }
        let writable = !p.call("GetSetMethod", &[]).is_null();
        members.push(member(o, &p.get("Name").to_string(), &p.get("PropertyType"), "property", truthy(&getter.get("IsStatic")), writable));
    }
    let fields = t.call("GetFields", &[]);
    for i in 0..fields.len().unwrap_or(0) {
        let f = fields.index(i);
        let writable = !truthy(&f.get("IsInitOnly")) && !truthy(&f.get("IsLiteral"));
        members.push(member(o, &f.get("Name").to_string(), &f.get("FieldType"), "field", truthy(&f.get("IsStatic")), writable));
    }
    members.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let count = o.len();
    let items: Vec<Value> = (0..count.unwrap_or(0).min(MAX_ITEMS))
        .map(|i| {
            let v = o.index(i);
            json!({ "index": i, "kind": kind(&v), "value": text(&v) })
        })
        .collect();
    json!({ "type": t.get("FullName").to_string(), "text": text(o), "count": count, "members": members, "items": items })
}

/// An indexer (`this[int]`) isn't a value to show.
fn props_indexed(p: &Obj) -> bool {
    p.call("GetIndexParameters", &[]).len().unwrap_or(0) > 0
}

/// A member's value. Some getters throw (not supported, not ready): those read as null, without filling the log.
fn read(o: &Obj, name: &str) -> Obj {
    match internal().get_quiet {
        Some(get) => unsafe { Obj::from_handle(get(o.handle(), name.as_ptr(), name.len())) },
        None => o.get(name),
    }
}

fn member(o: &Obj, name: &str, ty: &Obj, member: &str, is_static: bool, writable: bool) -> Value {
    let v = read(o, name);
    json!({
        "name": name, "member": member, "type": ty.get("Name").to_string(),
        "kind": kind(&v), "value": text(&v), "static": is_static, "writable": writable,
    })
}
