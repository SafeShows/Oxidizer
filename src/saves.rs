//! Data mods keep per save game, stored in the save's own folder as `<save>/oxidizer/<mod>.json` (a JSON object per
//! mod), so it travels with the save and goes when the save is deleted.
//!
//! When the game writes a save (`KSA.UncompressedSave.Write`), mods first get `saving()` to update their values,
//! then the files are written next to the game's. When it loads one (`UncompressedSave.Load`), the files are read
//! and mods get `save_loaded()`. A game started without loading a save starts with no data.

use crate::log;
use oxidizer_api::{Handle, Obj};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, Once};

static DATA: Mutex<BTreeMap<String, Map<String, Value>>> = Mutex::new(BTreeMap::new());
const FOLDER: &str = "oxidizer";

fn data() -> MutexGuard<'static, BTreeMap<String, Map<String, Value>>> {
    DATA.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn get(mod_id: &str, key: &str) -> Option<Value> {
    data().get(mod_id)?.get(key).cloned()
}

/// Sets a value; `null` removes it.
pub fn set(mod_id: &str, key: &str, value: Value) {
    let mut d = data();
    let map = d.entry(mod_id.to_string()).or_default();
    if value.is_null() {
        map.remove(key);
    } else {
        map.insert(key.to_string(), value);
    }
}

/// Hooks the game's saving and loading, once (after the game has loaded).
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let ty = "KSA.UncompressedSave, KSA";
        let ok = oxidizer_api::patch(ty, "Write", false, before_write, 0)
            & oxidizer_api::patch(ty, "Write", true, after_write, 0)
            & oxidizer_api::patch(ty, "Load", true, after_load, 0);
        if !ok {
            log("per-save data is off: the game's save methods couldn't be hooked (see the errors above)");
        }
    });
}

/// The save's folder, from the `UncompressedSave` a hook got.
fn folder(instance: Handle, args: Handle) -> Option<PathBuf> {
    let (save, _args) = unsafe { (Obj::from_handle(instance), Obj::from_handle(args)) }; // the hook owns both
    let dir = save.get("Directory").get("FullName").to_string();
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

extern "C" fn before_write(_: usize, instance: Handle, args: Handle) -> u8 {
    drop(folder(instance, args));
    crate::rust_mods::call_saving();
    crate::script::call_all("saving", ());
    1
}

extern "C" fn after_write(_: usize, instance: Handle, args: Handle) -> u8 {
    if let Some(dir) = folder(instance, args) {
        write(&dir.join(FOLDER), &data());
    }
    1
}

extern "C" fn after_load(_: usize, instance: Handle, args: Handle) -> u8 {
    if let Some(dir) = folder(instance, args) {
        *data() = read(&dir.join(FOLDER));
        crate::rust_mods::call_save_loaded();
        crate::script::call_all("save_loaded", ());
    }
    1
}

/// Replaces the folder's files with one per mod that has data.
fn write(dir: &Path, all: &BTreeMap<String, Map<String, Value>>) {
    let _ = std::fs::remove_dir_all(dir); // mods that no longer have data leave no file behind
    for (mod_id, map) in all.iter().filter(|(_, m)| !m.is_empty()) {
        let text = serde_json::to_string_pretty(map).unwrap_or_default();
        if let Err(e) = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(dir.join(format!("{mod_id}.json")), text)) {
            log(&format!("saving {mod_id}'s data to {}: {e}", dir.display()));
        }
    }
}

fn read(dir: &Path) -> BTreeMap<String, Map<String, Value>> {
    let mut all = BTreeMap::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let Some(mod_id) = path.file_stem().filter(|_| path.extension().is_some_and(|e| e == "json")) else { continue };
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Map<String, Value>>(&t).map_err(|e| e.to_string())) {
            Ok(map) => {
                all.insert(mod_id.to_string_lossy().into_owned(), map);
            }
            Err(e) => log(&format!("reading {}: {e}", path.display())),
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_goes_out_and_back() {
        let dir = std::env::temp_dir().join(format!("oxidizer_saves_test_{}", std::process::id()));
        let mut all = BTreeMap::new();
        let mut m = Map::new();
        m.insert("score".into(), Value::from(42));
        m.insert("name".into(), Value::from("Jeb"));
        all.insert("my_mod".to_string(), m);
        all.insert("empty_mod".to_string(), Map::new());
        write(&dir, &all);
        assert!(!dir.join("empty_mod.json").exists());
        let back = read(&dir);
        assert_eq!(back.len(), 1);
        assert_eq!(back["my_mod"]["score"], 42);
        assert_eq!(back["my_mod"]["name"], "Jeb");
        all.remove("my_mod");
        write(&dir, &all);
        assert!(read(&dir).is_empty(), "a mod without data leaves no file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn null_removes() {
        set("saves_test_mod", "a", Value::from(1));
        assert_eq!(get("saves_test_mod", "a"), Some(Value::from(1)));
        set("saves_test_mod", "a", Value::Null);
        assert_eq!(get("saves_test_mod", "a"), None);
    }
}
