//! Live values for Rhai Studio's graph editor: scripts it generates pass their wires' values through
//! `graph_probe(key, value)`, and the editor reads them with `graph_probes(source)`. Values are only recorded for
//! a script an editor read within the last few seconds, so a graph mod nobody is looking at pays one call per value.

use rhai::Dynamic;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a script's values keep being recorded after the editor last read them.
const KEEP: Duration = Duration::from_secs(3);

/// By source ("mod/file.rhai"): when the editor last read it, and the latest value text of each key.
static PROBES: Mutex<Option<HashMap<String, (Instant, HashMap<String, String>)>>> = Mutex::new(None);

pub fn record(source: &str, key: &str, v: &Dynamic) {
    let mut probes = PROBES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((read, values)) = probes.as_mut().and_then(|p| p.get_mut(source))
        && read.elapsed() < KEEP
    {
        values.insert(key.to_string(), crate::debug::text(v));
    }
}

/// The latest values of a script as a JSON object, `{"key": "value text"}`; starts or keeps recording them.
pub fn read(source: &str) -> String {
    let mut probes = PROBES.lock().unwrap_or_else(|e| e.into_inner());
    let (read, values) = probes.get_or_insert_default().entry(source.to_string()).or_insert_with(|| (Instant::now(), HashMap::new()));
    *read = Instant::now();
    serde_json::to_string(values).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn records_only_while_read() {
        super::record("m/a.rhai", "1.out", &1.into());
        assert_eq!(super::read("m/a.rhai"), "{}");
        super::record("m/a.rhai", "1.out", &2_i64.into());
        assert_eq!(super::read("m/a.rhai"), r#"{"1.out":"2"}"#);
    }
}
