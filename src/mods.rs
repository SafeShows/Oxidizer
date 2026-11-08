//! Finding Oxidizer mods, the order they start in, and their dependencies.
//!
//! ```toml
//! [oxidizer]
//! script = "main.rhai"
//! version = "1.2.0"                                   # this mod's version, for mods that depend on it
//! dependencies = [
//!     { id = "weather_core", min = "1.0", max = "1.9.9" },   # must be there (bounds inclusive, both optional)
//!     { id = "fancy_ui", optional = true },              # starts first if it is there
//! ]
//! load_after = ["some_mod"]                            # only the order, nothing required
//! ```
//!
//! Mods start in dependency order (ties by folder name, so every machine agrees). A mod whose required dependency
//! is missing, disabled, didn't start, or has a version outside the bounds doesn't start; its state says why.

use crate::{log, ui};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Clone, Debug)]
pub struct Dep {
    pub id: String,
    pub optional: bool,
    pub min: Option<String>,
    pub max: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub dir: PathBuf,
    pub dll: Option<String>,
    pub script: Option<PathBuf>,
    pub version: Option<String>,
    pub deps: Vec<Dep>,
    pub load_after: Vec<String>,
}

impl Entry {
    /// The kind shown in the mod list: Rust when the mod has a library.
    pub fn kind(&self) -> &'static str {
        if self.dll.is_some() { "Rust" } else { "Rhai" }
    }
}

/// Oxidizer mods that started, with their versions: what dependencies are checked against.
static STARTED: Mutex<BTreeMap<String, Option<String>>> = Mutex::new(BTreeMap::new());
/// Why each waiting mod doesn't start yet, so a reason is logged once, not every second.
static WAITING: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

pub fn mark_started(e: &Entry) {
    STARTED.lock().unwrap_or_else(|p| p.into_inner()).insert(e.id.clone(), e.version.clone());
    WAITING.lock().unwrap_or_else(|p| p.into_inner()).remove(&e.id);
}

/// The same, for a mod known by id (a script mod that started later, after its file was fixed).
pub fn mark_started_id(id: &str) {
    if let Some(e) = read(&oxidizer_api::mods_dir().join(id)) {
        mark_started(&e);
    }
}

/// Records why a mod doesn't start; logs it when the reason is new.
pub fn mark_waiting(e: &Entry, why: String) {
    let mut waiting = WAITING.lock().unwrap_or_else(|p| p.into_inner());
    if waiting.get(&e.id) != Some(&why) {
        log(&format!("{}: {why}", e.id));
        ui::set_status(&e.id, e.kind(), Some(why.clone()));
        waiting.insert(e.id.clone(), why);
    }
}

fn read_toml(path: &Path) -> Option<toml::Table> {
    fs::read_to_string(path).ok()?.parse().ok()
}

fn strings(v: Option<&toml::Value>) -> Vec<String> {
    v.and_then(|v| v.as_array()).into_iter().flatten().filter_map(|s| s.as_str().map(String::from)).collect()
}

/// A dependency: `"id"` or `{ id = "id", optional = true, min = "1.0", max = "2.0" }`.
fn dep(v: &toml::Value) -> Option<Dep> {
    if let Some(id) = v.as_str() {
        return Some(Dep { id: id.into(), optional: false, min: None, max: None });
    }
    let t = v.as_table()?;
    let s = |k: &str| t.get(k).and_then(|v| v.as_str()).map(String::from);
    Some(Dep { id: s("id")?, optional: t.get("optional").and_then(|v| v.as_bool()).unwrap_or(false), min: s("min"), max: s("max") })
}

/// The mod in `dir`, if it is an Oxidizer mod (its `mod.toml` has an `[oxidizer]` section).
pub fn read(dir: &Path) -> Option<Entry> {
    let cfg = read_toml(&dir.join("mod.toml"))?;
    let ox = cfg.get("oxidizer")?.as_table()?;
    Some(Entry {
        id: dir.file_name()?.to_string_lossy().into_owned(),
        dir: dir.to_path_buf(),
        dll: ox.get("dll").and_then(|v| v.as_str()).map(String::from),
        script: ox.get("script").and_then(|v| v.as_str()).map(|s| dir.join(s)),
        version: ox.get("version").and_then(|v| v.as_str()).map(String::from),
        deps: ox.get("dependencies").and_then(|v| v.as_array()).into_iter().flatten().filter_map(dep).collect(),
        load_after: strings(ox.get("load_after")),
    })
}

/// Mods turned off in the game's mod list (`enabled = false` in `manifest.toml`).
pub fn disabled_mods(manifest: &Path) -> HashSet<String> {
    let Some(t) = read_toml(manifest) else { return HashSet::new() };
    t.get("mods").and_then(|m| m.as_array()).into_iter().flatten()
        .filter(|m| m.get("enabled").and_then(|e| e.as_bool()) == Some(false))
        .filter_map(|m| m.get("id")?.as_str().map(String::from))
        .collect()
}

/// Every Oxidizer mod in the mods folder, except those disabled in the game's mod list.
pub fn scan(mods_dir: &Path) -> Vec<Entry> {
    let disabled = mods_dir.parent().map(|d| disabled_mods(&d.join("manifest.toml"))).unwrap_or_default();
    let Ok(dirs) = fs::read_dir(mods_dir) else { return vec![] };
    dirs.flatten().filter_map(|e| read(&e.path())).filter(|e| !disabled.contains(&e.id)).collect()
}

/// The order to start mods in: each after the mods it depends on or loads after (when they are there), ties by
/// folder name. Mods in a dependency cycle come last, in folder order; their ids are returned too.
pub fn order(mut entries: Vec<Entry>) -> (Vec<Entry>, Vec<String>) {
    entries.sort_by_key(|e| (e.id.to_lowercase(), e.id.clone()));
    let index: HashMap<String, usize> = entries.iter().enumerate().map(|(i, e)| (e.id.clone(), i)).collect();
    let mut after: Vec<Vec<usize>> = vec![vec![]; entries.len()]; // after[a] = mods that start after a
    let mut waiting_on = vec![0usize; entries.len()];
    for (i, e) in entries.iter().enumerate() {
        let before: HashSet<usize> = e.deps.iter().map(|d| &d.id).chain(&e.load_after).filter_map(|id| index.get(id)).copied().filter(|&j| j != i).collect();
        for j in before {
            after[j].push(i);
            waiting_on[i] += 1;
        }
    }
    let mut ready: std::collections::BTreeSet<usize> = (0..entries.len()).filter(|&i| waiting_on[i] == 0).collect();
    let mut sorted = vec![];
    while let Some(i) = ready.pop_first() {
        sorted.push(i);
        for &k in &after[i] {
            waiting_on[k] -= 1;
            if waiting_on[k] == 0 {
                ready.insert(k);
            }
        }
    }
    let cycle: Vec<usize> = (0..entries.len()).filter(|i| !sorted.contains(i)).collect();
    let cycle_ids = cycle.iter().map(|&i| entries[i].id.clone()).collect();
    sorted.extend(cycle);
    let mut slots: Vec<Option<Entry>> = entries.into_iter().map(Some).collect();
    (sorted.into_iter().map(|i| slots[i].take().unwrap()).collect(), cycle_ids)
}

/// Compares versions like "1.2.3", "v1.2" or "1.2.3-beta" by their numbers (missing parts are 0; a pre-release or
/// build suffix is ignored). None when one isn't a version.
pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    fn parts(v: &str) -> Option<Vec<u64>> {
        let core = v.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
        let mut n: Vec<u64> = core.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        n.resize(n.len().max(3), 0);
        Some(n)
    }
    Some(parts(a)?.cmp(&parts(b)?))
}

/// Whether `e` can start: every required dependency there and started (Oxidizer mods) or installed (other mods),
/// and every dependency that is there within its version bounds. `oxidizer` are the Oxidizer mods found.
pub fn check(e: &Entry, mods_dir: &Path, oxidizer: &HashSet<String>) -> Result<(), String> {
    let started = STARTED.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let disabled = mods_dir.parent().map(|d| disabled_mods(&d.join("manifest.toml"))).unwrap_or_default();
    for d in &e.deps {
        // The dependency's version, or why it isn't usable.
        let found: Result<Option<String>, String> = if disabled.contains(&d.id) {
            Err("disabled in the game's mod list".to_string())
        } else if oxidizer.contains(&d.id) {
            match started.get(&d.id) {
                Some(v) => Ok(v.clone()),
                None => Err("didn't start".to_string()),
            }
        } else if mods_dir.join(&d.id).join("mod.toml").exists() {
            Ok(read_toml(&mods_dir.join(&d.id).join("mod.toml"))
                .and_then(|t| t.get("oxidizer")?.get("version")?.as_str().map(String::from)))
        } else {
            Err("not installed".to_string())
        };
        let version = match found {
            Ok(v) => v,
            Err(_) if d.optional => continue,
            Err(why) => return Err(format!("needs {} ({why})", d.id)),
        };
        if d.min.is_none() && d.max.is_none() {
            continue;
        }
        let bounds = match (&d.min, &d.max) {
            (Some(lo), Some(hi)) => format!("{lo} to {hi}"),
            (Some(lo), None) => format!("{lo} or later"),
            (None, Some(hi)) => format!("up to {hi}"),
            (None, None) => unreachable!(),
        };
        let Some(v) = version else {
            return Err(format!("needs {} {bounds}, but {} doesn't say its version ([oxidizer] version)", d.id, d.id));
        };
        let fits = |bound: &Option<String>, bad: Ordering| match bound {
            None => Ok(true),
            Some(b) => compare_versions(&v, b).map(|o| o != bad).ok_or_else(|| format!("can't compare {} version {v} with {b}", d.id)),
        };
        if !(fits(&d.min, Ordering::Less)? && fits(&d.max, Ordering::Greater)?) {
            return Err(format!("needs {} {bounds}, but {v} is installed", d.id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, deps: &[&str], load_after: &[&str]) -> Entry {
        Entry {
            id: id.into(),
            dir: PathBuf::new(),
            dll: None,
            script: None,
            version: None,
            deps: deps.iter().map(|d| Dep { id: (*d).into(), optional: false, min: None, max: None }).collect(),
            load_after: load_after.iter().map(|s| (*s).into()).collect(),
        }
    }

    fn ids(v: &[Entry]) -> Vec<&str> {
        v.iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn orders_by_dependencies_then_name() {
        let (o, cycle) = order(vec![entry("c", &["a"], &[]), entry("b", &[], &[]), entry("a", &["missing"], &["z"]), entry("d", &[], &["c"])]);
        assert_eq!(ids(&o), ["a", "b", "c", "d"]);
        assert!(cycle.is_empty());
        let (o, _) = order(vec![entry("a", &["b"], &[]), entry("b", &[], &[])]);
        assert_eq!(ids(&o), ["b", "a"]);
    }

    #[test]
    fn cycles_go_last() {
        let (o, cycle) = order(vec![entry("x", &["y"], &[]), entry("y", &["x"], &[]), entry("z", &[], &[])]);
        assert_eq!(ids(&o), ["z", "x", "y"]);
        assert_eq!(cycle, ["x", "y"]);
    }

    #[test]
    fn versions() {
        assert_eq!(compare_versions("1.2", "1.2.0"), Some(Ordering::Equal));
        assert_eq!(compare_versions("v1.10.0", "1.9.9"), Some(Ordering::Greater));
        assert_eq!(compare_versions("2.0.0-beta", "2.0.0"), Some(Ordering::Equal));
        assert_eq!(compare_versions("1.2.3.4", "1.2.3"), Some(Ordering::Greater));
        assert_eq!(compare_versions("one", "1.0"), None);
    }
}
