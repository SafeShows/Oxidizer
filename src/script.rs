//! Rhai script mods. Each script may define `init()`, `loaded()` (the game finished loading), `frame(t, dt)`,
//! `gui(dt)`, `menu()`, `unload()`, and `reload()`, which runs when the file was changed and reloaded while the
//! game runs.
//! Per-mod state lives in `this` (an object map) and survives reloads,
//! e.g. `this.show = checkbox("Show", this.show ?? false);`.

use crate::{log, ui};
use oxidizer_api::{Handle, Obj, imgui};
use rhai::{Array, CallFnOptions, Dynamic, Engine, EvalAltResult, FuncArgs, FuncRegistration, Map, RhaiNativeFunc, Scope, Variant, AST};
use std::cell::Cell;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, TryLockError};
use std::time::{Duration, Instant, SystemTime};

const KIND: &str = "Rhai";
const RELOAD_CHECK: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub(crate) struct RObj(pub(crate) Arc<Obj>);

impl RObj {
    pub(crate) fn new(o: Obj) -> RObj {
        RObj(Arc::new(o))
    }
}

struct Script {
    name: String,
    path: PathBuf,
    modified: Option<SystemTime>,
    ast: AST,
    scope: Scope<'static>,
    this: Dynamic,
    /// Stopped after an error, until the file changes.
    dead: bool,
    /// init() has run without error.
    started: bool,
}

/// The engine is only read after setup, so all scripts share it; each script has its own lock, so a script can
/// call a game method that another script hooks while it runs.
struct Scripts {
    engine: Engine,
    /// Only grows (a script keeps its index, which `CURRENT` and hooks use). Read through `mods()`.
    mods: Mutex<Vec<Arc<Mutex<Script>>>>,
    /// Their names, readable without waiting for a busy script.
    names: Mutex<Vec<String>>,
    /// Variables of the console (`eval`), kept between calls.
    console: Mutex<Scope<'static>>,
}

static SCRIPTS: OnceLock<Scripts> = OnceLock::new();

/// The script engine, built with typed game access on first use (when the first script mod loads, or the console runs).
fn scripts() -> &'static Scripts {
    SCRIPTS.get_or_init(|| {
        let started = Instant::now();
        let mut engine = engine();
        let count = crate::ksa_rhai::register(&mut engine);
        log(&format!("typed game access for Rhai: {count} functions in {} ms", started.elapsed().as_millis()));
        let docs = oxidizer_api::config_path("x").parent().map(Path::to_path_buf).unwrap_or_default();
        crate::ksa_rhai::write_definitions(&engine, &docs.join("definitions"));
        Scripts { engine, mods: Mutex::new(vec![]), names: Mutex::new(vec![]), console: Mutex::new(Scope::new()) }
    })
}

/// The loaded scripts, copied out so no list lock is held while they run (they may add scripts or call hooks).
fn mods() -> Vec<Arc<Mutex<Script>>> {
    SCRIPTS.get().map(|s| s.mods.lock().unwrap_or_else(|e| e.into_inner()).clone()).unwrap_or_default()
}

thread_local! {
    /// The script running on this thread, so `patch()` and `config_get()` know who calls them.
    static CURRENT: Cell<usize> = const { Cell::new(0) };
}

/// A script's lock, or None when it is busy: running further up this thread (a hook it set on a method it
/// called itself) or, briefly, on another thread. Busy scripts are skipped rather than waited for.
fn lock(m: &Mutex<Script>) -> Option<MutexGuard<'_, Script>> {
    match m.try_lock() {
        Ok(g) => Some(g),
        Err(TryLockError::Poisoned(p)) => Some(p.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

/// Saved settings of each script (same index as `Scripts::mods`), written at most once a second.
struct Config {
    path: PathBuf,
    table: toml::Table,
    dirty: bool,
}

static CONFIGS: Mutex<Vec<Config>> = Mutex::new(Vec::new());

/// The editing buffers of Rhai text fields, by script and label: ImGui edits text in place, across frames.
static TEXT_FIELDS: Mutex<Vec<((usize, String), imgui::TextBuffer)>> = Mutex::new(Vec::new());

/// Draws a text field over `text` and returns the text after this frame's edits. The field keeps its own buffer;
/// it takes `text` again whenever the script passes something else (e.g. it reset the value).
fn text_field(label: &str, text: &str, draw: impl FnOnce(&mut imgui::TextBuffer)) -> String {
    let key = (CURRENT.get(), label.to_string());
    let mut fields = TEXT_FIELDS.lock().unwrap_or_else(|e| e.into_inner());
    let i = match fields.iter().position(|(k, _)| *k == key) {
        Some(i) => i,
        None => {
            fields.push((key, imgui::TextBuffer::new(text)));
            fields.len() - 1
        }
    };
    let buf = &mut fields[i].1;
    if buf.as_str() != text {
        buf.set(text);
    }
    draw(buf);
    buf.as_str().to_string()
}

/// A color from a script: [r, g, b] or [r, g, b, a], each 0..1 (integers count too).
fn rgba(c: &Array) -> [f32; 4] {
    let at = |i: usize, d: f32| c.get(i).and_then(|v| v.as_float().ok().or(v.as_int().ok().map(|n| n as f64))).map_or(d, |v| v as f32);
    [at(0, 1.0), at(1, 1.0), at(2, 1.0), at(3, 1.0)]
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Compile the file and run its top-level code. Its source name is "mod/file.rhai" (breakpoints use it).
fn compile(engine: &Engine, path: &Path, name: &str) -> Result<(AST, Scope<'static>), String> {
    let mut scope = Scope::new();
    let mut ast = engine.compile_file(path.into()).map_err(|e| e.to_string())?;
    let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    ast.set_source(format!("{name}/{file}"));
    engine.run_ast_with_scope(&mut scope, &ast).map_err(|e| e.to_string())?;
    Ok((ast, scope))
}

fn report(name: &str, problem: String) {
    log(&format!("{name}: {problem}"));
    ui::set_status(name, KIND, Some(problem));
}

/// Loads one script mod and runs its init(); true when it started without an error. The engine is built when the
/// first script mod (or the console) needs it. A script that fails stays in the list: saving a fixed file starts it.
pub fn start_mod(name: &str, path: &Path) -> bool {
    let s = scripts();
    let i = add(s, name.to_string(), path.to_path_buf());
    let Some(script) = mods().get(i).cloned() else { return false };
    let Some(mut m) = lock(&script) else { return false };
    start(&s.engine, i, &mut m);
    m.started
}

/// Compiles a script mod and adds it (with its settings) to the list; returns its index. A script that doesn't
/// compile stays in the list, so fixing the file starts it.
fn add(s: &Scripts, name: String, path: PathBuf) -> usize {
    let modified = modified(&path);
    let (ast, scope, dead) = match compile(&s.engine, &path, &name) {
        Ok((ast, scope)) => {
            log(&format!("loaded script mod {name}"));
            (ast, scope, false)
        }
        Err(e) => {
            report(&name, e);
            (AST::empty(), Scope::new(), true)
        }
    };
    let config = oxidizer_api::config_path(&name);
    let table = fs::read_to_string(&config).ok().and_then(|t| t.parse().ok()).unwrap_or_default();
    // Same index in both lists.
    CONFIGS.lock().unwrap_or_else(|e| e.into_inner()).push(Config { path: config, table, dirty: false });
    s.names.lock().unwrap_or_else(|e| e.into_inner()).push(name.clone());
    let mut mods = s.mods.lock().unwrap_or_else(|e| e.into_inner());
    mods.push(Arc::new(Mutex::new(Script { name, path, modified, ast, scope, this: Map::new().into(), dead, started: false })));
    mods.len() - 1
}

/// Starts script mods that appeared in the mods folder while the game runs (e.g. made by an in-game editor).
/// Also starts script mods whose dependencies were missing, once they are met.
fn start_new() {
    let known = SCRIPTS.get().map(|s| s.names.lock().unwrap_or_else(|e| e.into_inner()).clone()).unwrap_or_default();
    let mods_dir = oxidizer_api::mods_dir();
    let all = crate::mods::scan(&mods_dir);
    let oxidizer: HashSet<String> = all.iter().map(|e| e.id.clone()).collect();
    let (ordered, _) = crate::mods::order(all);
    // Only Rhai mods: Rust mods load when the game starts.
    for e in ordered.iter().filter(|e| e.dll.is_none() && !known.contains(&e.id)) {
        let Some(script) = &e.script else { continue };
        if let Err(why) = crate::mods::check(e, &mods_dir, &oxidizer) {
            crate::mods::mark_waiting(e, why);
            continue;
        }
        log(&format!("new script mod {}", e.id));
        start_mod(&e.id, script);
    }
}

// ---- the event bus and services (see bus.rs) ----

fn index_of(script: &str) -> Option<usize> {
    SCRIPTS.get()?.names.lock().unwrap_or_else(|e| e.into_inner()).iter().position(|n| n == script)
}

/// The mod whose code runs on this thread: a script mod's id, or "console" for the console.
fn caller_name() -> Option<String> {
    let i = CURRENT.get();
    if i == usize::MAX {
        return Some("console".into());
    }
    SCRIPTS.get()?.names.lock().unwrap_or_else(|e| e.into_inner()).get(i).cloned()
}

fn json_to_dyn(json: &str) -> Dynamic {
    serde_json::from_str::<serde_json::Value>(json).ok().and_then(|v| rhai::serde::to_dynamic(v).ok()).unwrap_or(Dynamic::UNIT)
}

fn dyn_to_json(d: &Dynamic) -> Result<String, String> {
    rhai::serde::from_dynamic::<serde_json::Value>(d)
        .map(|v| v.to_string())
        .map_err(|e| format!("only plain values (numbers, strings, bools, arrays, maps) can be sent: {e}"))
}

/// Gives a script its event. Its error stops the script, like an error in frame().
pub fn deliver_event(script: &str, function: &str, json: &str) {
    let (Some(s), Some(i)) = (SCRIPTS.get(), index_of(script)) else { return };
    let Some(m) = mods().get(i).cloned() else { return };
    let Some(mut m) = lock(&m) else {
        log(&format!("{script}: busy, missed an event for {function}"));
        return;
    };
    call(&s.engine, i, &mut m, function, (json_to_dyn(json),));
}

/// Runs a script's service function: its result as JSON, or the error (which doesn't stop the script).
pub fn call_service(script: &str, function: &str, args: &str) -> Result<String, String> {
    let (Some(s), Some(i)) = (SCRIPTS.get(), index_of(script)) else { return Err(format!("{script} isn't loaded")) };
    let m = mods().get(i).cloned().ok_or_else(|| format!("{script} isn't loaded"))?;
    let mut m = lock(&m).ok_or_else(|| format!("{script} is busy (running further up this call)"))?;
    if m.dead {
        return Err(format!("{script} has stopped after an error"));
    }
    if crate::debug::paused(script) {
        return Err(format!("{script} is paused at a breakpoint"));
    }
    let outer = CURRENT.replace(i);
    let m = &mut *m;
    let opts = CallFnOptions::new().eval_ast(false).bind_this_ptr(&mut m.this);
    let result = crate::perf::time(&m.name, || s.engine.call_fn_with_options::<Dynamic>(opts, &mut m.scope, &m.ast, function, (json_to_dyn(args),)));
    CURRENT.set(outer);
    dyn_to_json(&result.map_err(|e| e.to_string())?)
}

/// Runs Rhai code like a console line: variables persist between calls, `print` goes to the log.
/// Returns the value as text (empty for `()`), or the error.
pub fn eval(code: &str) -> Result<String, String> {
    let s = scripts();
    let mut scope = s.console.lock().unwrap_or_else(|e| e.into_inner());
    let outer = CURRENT.replace(usize::MAX); // no script: config_get/set and patch() find no mod
    let result = s.engine.eval_with_scope::<Dynamic>(&mut scope, code);
    CURRENT.set(outer);
    match result {
        Ok(v) if v.is_unit() => Ok(String::new()),
        Ok(v) => Ok(match v.clone().try_cast::<RObj>() {
            Some(o) => o.0.to_string(),
            None => v.to_string(),
        }),
        Err(e) => Err(e.to_string()),
    }
}

/// Runs init() once per script.
fn start(engine: &Engine, i: usize, m: &mut Script) {
    if m.dead {
        return;
    }
    call(engine, i, m, "init", ());
    m.started = !m.dead;
    if m.started {
        ui::set_status(&m.name, KIND, None);
        crate::mods::mark_started_id(&m.name); // mods that depend on it can start now
        if crate::LOADED.load(std::sync::atomic::Ordering::Relaxed) {
            call(engine, i, m, "loaded", ()); // started after the game loaded: it still gets loaded()
        }
    }
}

/// Reloads scripts whose file changed (checked once a second). `this` is kept; a script that
/// never started runs init(), the others run reload(). Hooks call the new code by name.
pub fn reload_changed() {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|t| t.elapsed() < RELOAD_CHECK) {
            return;
        }
        *last = Some(Instant::now());
    }
    save_settings();
    start_new();
    let Some(s) = SCRIPTS.get() else { return };
    for (i, m) in mods().iter().enumerate() {
        let Some(mut m) = lock(m) else { continue };
        let t = modified(&m.path);
        if t == m.modified {
            continue;
        }
        m.modified = t;
        match compile(&s.engine, &m.path, &m.name) {
            Ok((ast, scope)) => {
                (m.ast, m.scope, m.dead) = (ast, scope, false);
                log(&format!("reloaded script mod {}", m.name));
                ui::set_status(&m.name, KIND, None);
                if m.started {
                    call(&s.engine, i, &mut m, "reload", ());
                } else {
                    start(&s.engine, i, &mut m);
                }
            }
            // The previous version keeps running (unless it had stopped).
            Err(e) => report(&m.name.clone(), format!("reload failed: {e}")),
        }
    }
}

pub fn call_all(f: &str, args: impl FuncArgs + Clone) {
    let Some(s) = SCRIPTS.get() else { return };
    for (i, m) in mods().iter().enumerate() {
        if let Some(mut m) = lock(m) {
            call(&s.engine, i, &mut m, f, args.clone());
        }
    }
}

fn call(engine: &Engine, i: usize, m: &mut Script, f: &str, args: impl FuncArgs) -> Option<Dynamic> {
    if m.dead || !m.ast.iter_functions().any(|x| x.name == f) {
        return None;
    }
    if !matches!(f, "init" | "reload" | "unload") && crate::debug::paused(&m.name) {
        return None; // stopped at a breakpoint: frames, windows, events and hooks wait for "continue"
    }
    let outer = CURRENT.replace(i); // calls nest when a script calls a method another script hooks
    let opts = CallFnOptions::new().eval_ast(false).bind_this_ptr(&mut m.this);
    let result = crate::perf::time(&m.name, || engine.call_fn_with_options::<Dynamic>(opts, &mut m.scope, &m.ast, f, args));
    CURRENT.set(outer);
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            // Errors in frame/gui would repeat every frame: stop the script until its file changes.
            m.dead = true;
            report(&m.name, format!("{f}: {e} (stopped until the file is saved again)"));
            None
        }
    }
}

/// Write settings that changed since the last save.
pub fn save_settings() {
    for c in CONFIGS.lock().unwrap_or_else(|e| e.into_inner()).iter_mut().filter(|c| c.dirty) {
        c.dirty = false;
        if let Err(e) = fs::write(&c.path, c.table.to_string()) {
            log(&format!("saving {}: {e}", c.path.display()));
        }
    }
}

fn config_get(key: &str, default: Dynamic) -> Dynamic {
    let configs = CONFIGS.lock().unwrap_or_else(|e| e.into_inner());
    match configs.get(CURRENT.get()).and_then(|c| c.table.get(key)) {
        Some(toml::Value::String(v)) => v.clone().into(),
        Some(toml::Value::Integer(v)) => (*v).into(),
        Some(toml::Value::Float(v)) => (*v).into(),
        Some(toml::Value::Boolean(v)) => (*v).into(),
        _ => default,
    }
}

fn config_set(key: &str, v: Dynamic) -> Result<(), Box<EvalAltResult>> {
    let value = if let Ok(b) = v.as_bool() {
        toml::Value::Boolean(b)
    } else if let Ok(i) = v.as_int() {
        toml::Value::Integer(i)
    } else if let Ok(f) = v.as_float() {
        toml::Value::Float(f)
    } else if v.is_string() {
        toml::Value::String(v.into_string()?)
    } else {
        return Err(format!("config_set: '{key}' must be a bool, number or string").into());
    };
    let mut configs = CONFIGS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(c) = configs.get_mut(CURRENT.get()) {
        if c.table.get(key) != Some(&value) {
            c.table.insert(key.into(), value);
            c.dirty = true;
        }
    }
    Ok(())
}

extern "C" fn hook_tramp(user: usize, inst: Handle, args: Handle) -> u8 {
    let (inst, args) = unsafe { (RObj::new(Obj::from_handle(inst)), RObj::new(Obj::from_handle(args))) };
    let (i, f) = unsafe { &*(user as *const (usize, String)) };
    let Some(s) = SCRIPTS.get() else { return 1 };
    let Some(script) = mods().get(*i).cloned() else { return 1 }; // the console patched it: no script to call
    let Some(mut m) = lock(&script) else { return 1 }; // the script is busy: see lock()
    call(&s.engine, *i, &mut m, f, (inst, args)).and_then(|v| v.as_bool().ok()).unwrap_or(true) as u8
}

pub(crate) fn to_obj(d: &Dynamic) -> Arc<Obj> {
    if let Some(o) = d.clone().try_cast::<RObj>() {
        return o.0;
    }
    Arc::new(if let Ok(f) = d.as_float() {
        f.into()
    } else if let Ok(i) = d.as_int() {
        (i as f64).into()
    } else if let Ok(b) = d.as_bool() {
        (b as u8 as f64).into()
    } else if d.is_string() {
        d.clone().into_string().unwrap().as_str().into()
    } else {
        Obj::NULL
    })
}

fn args(a: &Array) -> Vec<Arc<Obj>> {
    a.iter().map(to_obj).collect()
}

fn refs(v: &[Arc<Obj>]) -> Vec<&Obj> {
    v.iter().map(|o| &**o).collect()
}

/// `register_fn` with what editors show: parameter names and types (then the return type), and a doc line.
trait Def {
    fn def<A: 'static, const N: usize, const X: bool, R: Variant + Clone, const F: bool>(
        &mut self,
        name: &str,
        info: &[&str],
        doc: &str,
        f: impl RhaiNativeFunc<A, N, X, R, F> + Send + Sync + 'static,
    ) -> &mut Self;
}

impl Def for Engine {
    fn def<A: 'static, const N: usize, const X: bool, R: Variant + Clone, const F: bool>(
        &mut self,
        name: &str,
        info: &[&str],
        doc: &str,
        f: impl RhaiNativeFunc<A, N, X, R, F> + Send + Sync + 'static,
    ) -> &mut Self {
        FuncRegistration::new(name).with_params_info(info).with_comments([format!("/// {doc}")]).register_into_engine(self, f);
        self
    }
}

fn engine() -> Engine {
    let mut e = Engine::new();
    e.on_print(|s| log(s));
    e.on_debug(|s, _, _| log(s));

    // game objects
    e.register_type_with_name::<RObj>("Obj")
        .def("static_get", &["type_name: string", "member: string", "Obj"], "A static property or field of a game type, by name: `static_get(\"KSA.Program\", \"ControlledVehicle\")`.", |ty: &str, m: &str| RObj::new(Obj::static_get(ty, m)))
        .def("static_set", &["type_name: string", "member: string", "value: ?", "bool"], "Sets a static property or field of a game type; false if that failed.", |ty: &str, m: &str, v: Dynamic| Obj::static_set(ty, m, &to_obj(&v)))
        .def("static_call", &["type_name: string", "method: string", "args: array", "Obj"], "Calls a static method of a game type. The method can name its parameter types: `\"SetSimulationSpeed(double, bool)\"`.", |ty: &str, m: &str, a: Array| {
            let a = args(&a);
            RObj::new(Obj::static_call(ty, m, &refs(&a)))
        })
        .def("static_call", &["type_name: string", "method: string", "Obj"], "Calls a static method of a game type that takes no arguments.", |ty: &str, m: &str| RObj::new(Obj::static_call(ty, m, &[])))
        .def("get", &["obj: Obj", "member: string", "Obj"], "A property or field of a game object, by name.", |o: &mut RObj, m: &str| RObj::new(o.0.get(m)))
        .def("set", &["obj: Obj", "member: string", "value: ?", "bool"], "Sets a property or field of a game object; false if that failed.", |o: &mut RObj, m: &str, v: Dynamic| o.0.set(m, &to_obj(&v)))
        // `invoke`, not `call`: Rhai keeps `x.call(...)` for function pointers.
        .def("invoke", &["obj: Obj", "method: string", "args: array", "Obj"], "Calls a method of a game object, by name.", |o: &mut RObj, m: &str, a: Array| {
            let a = args(&a);
            RObj::new(o.0.call(m, &refs(&a)))
        })
        .def("invoke", &["obj: Obj", "method: string", "Obj"], "Calls a method of a game object that takes no arguments.", |o: &mut RObj, m: &str| RObj::new(o.0.call(m, &[])))
        .def("float", &["obj: Obj", "?"], "The object as a number, or () if it isn't one.", |o: &mut RObj| o.0.f64().map_or(Dynamic::UNIT, Dynamic::from))
        .def("is_null", &["obj: Obj", "bool"], "Whether the object is null (nothing).", |o: &mut RObj| o.0.is_null())
        .def("len", &["obj: Obj", "int"], "The number of items in a game collection, or -1 if it isn't one.", |o: &mut RObj| o.0.len().map_or(-1, |n| n as i64))
        .def("to_string", &["obj: Obj", "string"], "The object as text (its C# ToString).", |o: &mut RObj| o.0.to_string())
        .def("to_debug", &["obj: Obj", "string"], "The object as text (its C# ToString).", |o: &mut RObj| o.0.to_string());
    FuncRegistration::new_index_getter()
        .with_params_info(["obj: Obj", "index: int", "Obj"])
        .with_comments(["/// An item of a game collection: `obj[0]`."])
        .register_into_engine(&mut e, |o: &mut RObj, i: i64| RObj::new(o.0.index(i as usize)));

    e.def("patch", &["type_name: string", "method: string", "kind: string", "function: string", "bool"],
        "Hooks a game method: kind is \"prefix\" or \"postfix\", and `function(instance, args)` in this script runs before or after it; a prefix that returns false skips the original.",
        |ty: &str, m: &str, kind: &str, f: &str| {
            let user = Box::leak(Box::new((CURRENT.get(), f.to_string()))) as *const _ as usize;
            oxidizer_api::patch(ty, m, kind == "postfix", hook_tramp, user)
        });

    // inside a hook: the hooked method's return value; set_result replaces it
    e.def("result", &["Obj"], "Inside a hook: the hooked method's return value.", || RObj::new(oxidizer_api::hook_result()))
        .def("set_result", &["value: ?", "bool"], "Inside a hook: replaces the hooked method's return value.", |v: Dynamic| oxidizer_api::set_hook_result(&to_obj(&v)));

    // the event bus and services: names start with this mod's id; values are plain Rhai values (maps, arrays, ...)
    let who = || caller_name().ok_or_else(|| Box::<EvalAltResult>::from("no mod is running"));
    let script_who = move || match caller_name() {
        Some(n) if n != "console" => Ok(n),
        _ => Err(Box::<EvalAltResult>::from("subscribe and provide only work from a mod's script")),
    };
    e.def("publish", &["topic: string", "value: ?", "()"], "Sends value to the topic's subscribers, at the start of the next frame. Topics start with your mod's id.", move |t: &str, v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        Ok(crate::bus::publish(&who()?, t, &dyn_to_json(&v)?, false)?)
    })
    .def("publish_retained", &["topic: string", "value: ?", "()"], "Like publish, and keeps value as the topic's current value for mods that subscribe later.", move |t: &str, v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        Ok(crate::bus::publish(&who()?, t, &dyn_to_json(&v)?, true)?)
    })
    .def("subscribe", &["topic: string", "function: string", "()"], "Calls function(value) in this script for each event on the topic (and its retained value, if any).", move |t: &str, f: &str| -> Result<(), Box<EvalAltResult>> {
        crate::bus::subscribe(t, crate::bus::Target::Rhai { script: script_who()?, function: f.into() })?;
        Ok(())
    })
    .def("unsubscribe", &["topic: string", "function: string", "()"], "Stops calling function for the topic's events.", move |t: &str, f: &str| -> Result<(), Box<EvalAltResult>> {
        crate::bus::unsubscribe_target(t, &crate::bus::Target::Rhai { script: script_who()?, function: f.into() });
        Ok(())
    })
    .def("provide", &["name: string", "function: string", "()"], "Lets other mods call function(args) in this script as the service name; its return value is the result. Names start with your mod's id.", move |n: &str, f: &str| -> Result<(), Box<EvalAltResult>> {
        Ok(crate::bus::provide(&script_who()?, n, crate::bus::Provider::Rhai { script: script_who()?, function: f.into() })?)
    })
    .def("call_service", &["name: string", "args: ?", "?"], "Calls a service another mod provides and returns its value; throws its error.", |n: &str, args: Dynamic| -> Result<Dynamic, Box<EvalAltResult>> {
        Ok(json_to_dyn(&crate::bus::call(n, &dyn_to_json(&args)?)?))
    })
    .def("call_service", &["name: string", "?"], "Calls a service that takes no arguments and returns its value; throws its error.", |n: &str| -> Result<Dynamic, Box<EvalAltResult>> {
        Ok(json_to_dyn(&crate::bus::call(n, "null")?))
    });

    // keys, by the game's key names ("F5", "K", "Space"); pressed = this frame only
    // breakpoints (Rhai Studio uses these; see debug.rs)
    e.def("debug_breakpoints", &["source: string", "lines: array", "()"], "Sets the breakpoint lines of a script, named \"mod/file.rhai\"; an empty array clears them. A mod that reaches one is paused until debug_continue.", |source: &str, lines: Array| {
        crate::debug::set_breakpoints(source, lines.iter().filter_map(|l| l.as_int().ok()).filter(|&l| l > 0).map(|l| l as usize).collect());
    })
    .def("debug_continue", &["mod_id: string", "()"], "Lets a mod paused at a breakpoint run again.", |m: &str| crate::debug::resume(m))
    .def("debug_state", &["string"], "Breakpoints, and what each one saw when last hit (variables, this, call stack), as JSON.", crate::debug::state);
    crate::debug::install(&mut e);

    e.def("key_pressed", &["key: string", "bool"], "True on the frame the key was pressed. Names are the game's: \"F5\", \"K\", \"Space\", \"LeftShift\".", |k: &str| oxidizer_api::key_pressed(k))
        .def("key_down", &["key: string", "bool"], "True while the key is held.", |k: &str| oxidizer_api::key_down(k));

    // settings saved per mod in <documents>/oxidizer/<mod>.toml: bools, numbers and strings
    e.def("config_get", &["key: string", "default: ?", "?"], "A saved setting of this mod, or default. Settings are bools, numbers and strings.", config_get)
        .def("config_set", &["key: string", "value: ?", "()"], "Saves a setting of this mod (to <documents>/oxidizer/<mod>.toml).", config_set);

    // imgui (only inside gui())
    e.def("begin", &["title: string", "bool"], "Starts a window (in gui()); true while it is open. Always call end().", |t: &str| imgui::begin(t))
        .def("end", &["()"], "Ends the window begin() started.", imgui::end)
        .def("text", &["text: string", "()"], "A line of text.", |s: &str| imgui::text(s))
        .def("button", &["label: string", "bool"], "A button; true when clicked.", |s: &str| imgui::button(s))
        .def("checkbox", &["label: string", "value: bool", "bool"], "A checkbox; returns the new value.", |l: &str, mut v: bool| {
            imgui::checkbox(l, &mut v);
            v
        })
        .def("slider", &["label: string", "value: float", "min: float", "max: float", "float"], "A slider; returns the new value.", |l: &str, v: f64, min: f64, max: f64| {
            let mut x = v as f32;
            imgui::slider_float(l, &mut x, min as f32, max as f32);
            x as f64
        })
        .def("same_line", &["()"], "Puts the next item on the same line.", imgui::same_line)
        .def("separator", &["()"], "A horizontal line.", imgui::separator)
        .def("header", &["label: string", "bool"], "A collapsing section header; true while open.", |l: &str| imgui::collapsing_header(l))
        .def("text_colored", &["color: array", "text: string", "()"], "Text in a color: [r, g, b] or [r, g, b, a], 0.0 to 1.0.", |color: Array, s: &str| imgui::text_colored(rgba(&color), s))
        .def("input_text", &["label: string", "text: string", "string"], "A one-line text field; returns the new text.", |l: &str, text: &str| text_field(l, text, |b| {
            imgui::input_text(l, b, 0);
        }))
        .def("input_multiline", &["label: string", "text: string", "width: float", "height: float", "string"], "A multi-line text field; returns the new text.", |l: &str, text: &str, w: f64, h: f64| text_field(l, text, |b| {
            imgui::input_text_multiline(l, b, w as f32, h as f32, imgui::text_flags::ALLOW_TAB_INPUT);
        }))
        .def("input_float", &["label: string", "value: float", "float"], "A number field; returns the new value.", |l: &str, mut v: f64| {
            imgui::input_double(l, &mut v);
            v
        })
        .def("begin_tab_bar", &["id: string", "bool"], "Starts a row of tabs; call end_tab_bar() only when this returned true.", |id: &str| imgui::begin_tab_bar(id))
        .def("end_tab_bar", &["()"], "Ends the tab bar.", imgui::end_tab_bar)
        .def("begin_tab", &["label: string", "bool"], "A tab; true while it is the selected one. Call end_tab() only then.", |l: &str| imgui::begin_tab_item(l, None, 0))
        .def("end_tab", &["()"], "Ends the tab.", imgui::end_tab_item)
        .def("begin_child", &["id: string", "height: float", "bool"], "A scrolling region, height pixels tall (0 fills the window). Always call end_child().", |id: &str, h: f64| imgui::begin_child(id, h as f32))
        .def("end_child", &["()"], "Ends the scrolling region.", imgui::end_child)
        .def("set_next_window_size", &["width: float", "height: float", "()"], "The next window's size when it first appears.", |w: f64, h: f64| imgui::set_next_window_size(w as f32, h as f32))
        .def("ctrl_shortcut", &["key: string", "bool"], "True on the frame Ctrl and the letter were pressed, also while typing in a text field.", |k: &str| k.chars().next().is_some_and(imgui::ctrl_shortcut));

    // menu entries (only inside menu(), which runs inside the "Mods" menu)
    e.def("menu_item", &["label: string", "bool"], "An entry in the Mods menu (in menu()); true when clicked.", |l: &str| imgui::menu_item(l, false))
        .def("menu_item", &["label: string", "selected: bool", "bool"], "An entry with a check mark while selected; true when clicked.", |l: &str, selected: bool| imgui::menu_item(l, selected))
        .def("begin_menu", &["label: string", "bool"], "A submenu; call end_menu() only when this returned true.", |l: &str| imgui::begin_menu(l))
        .def("end_menu", &["()"], "Ends the submenu.", imgui::end_menu);
    e
}

#[cfg(test)]
mod tests {
    #[test]
    fn definitions_name_parameters() {
        let defs = super::engine().definitions().single_file();
        for want in [
            "fn subscribe(topic: string, function: string) -> ();",
            "fn call_service(name: string, args: ?) -> ?;",
            "fn static_get(type_name: string, member: string) -> Obj;",
            "fn text(text: string) -> ();",
            "/// Calls function(value) in this script",
        ] {
            assert!(defs.contains(want), "missing {want:?}");
        }
        assert!(!defs.contains("EvalAltResult"));
        // Rhai's own to_float and to_int come without names: not ours to fix.
        let bare: Vec<&str> = defs.lines().filter(|l| l.contains("(_: ") && !l.starts_with("fn to_")).collect();
        assert!(bare.is_empty(), "without parameter names: {bare:?}");
    }
}
