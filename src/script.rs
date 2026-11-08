//! Rhai script mods. Each script may define `init()`, `frame(t, dt)`, `gui(dt)`, `menu()`, `unload()`,
//! and `reload()`, which runs when the file was changed and reloaded while the game runs.
//! Per-mod state lives in `this` (an object map) and survives reloads,
//! e.g. `this.show = checkbox("Show", this.show ?? false);`.

use crate::{log, ui};
use oxidizer_api::{Handle, Obj, imgui};
use rhai::{Array, CallFnOptions, Dynamic, Engine, EvalAltResult, FuncArgs, Map, Scope, AST};
use std::cell::Cell;
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

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Compile the file and run its top-level code.
fn compile(engine: &Engine, path: &Path) -> Result<(AST, Scope<'static>), String> {
    let mut scope = Scope::new();
    let ast = engine.compile_file(path.into()).map_err(|e| e.to_string())?;
    engine.run_ast_with_scope(&mut scope, &ast).map_err(|e| e.to_string())?;
    Ok((ast, scope))
}

fn report(name: &str, problem: String) {
    log(&format!("{name}: {problem}"));
    ui::set_status(name, KIND, Some(problem));
}

pub fn load(list: Vec<(String, PathBuf)>) {
    if list.is_empty() {
        return; // the engine is built when a script mod or the console first needs it
    }
    let s = scripts();
    let added: Vec<usize> = list.into_iter().map(|(name, path)| add(s, name, path)).collect();
    let mods = mods();
    for i in added {
        if let Some(mut m) = lock(&mods[i]) {
            start(&s.engine, i, &mut m);
        }
    }
}

/// Compiles a script mod and adds it (with its settings) to the list; returns its index. A script that doesn't
/// compile stays in the list, so fixing the file starts it.
fn add(s: &Scripts, name: String, path: PathBuf) -> usize {
    let modified = modified(&path);
    let (ast, scope, dead) = match compile(&s.engine, &path) {
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
fn start_new() {
    let known = SCRIPTS.get().map(|s| s.names.lock().unwrap_or_else(|e| e.into_inner()).clone()).unwrap_or_default();
    for (name, path) in crate::script_mods().into_iter().filter(|(n, _)| !known.contains(n)) {
        let s = scripts();
        log(&format!("new script mod {name}"));
        let i = add(s, name, path);
        if let Some(mut m) = mods().get(i).and_then(|m| lock(m)) {
            start(&s.engine, i, &mut m);
        }
    }
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
        match compile(&s.engine, &m.path) {
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
    let outer = CURRENT.replace(i); // calls nest when a script calls a method another script hooks
    let opts = CallFnOptions::new().eval_ast(false).bind_this_ptr(&mut m.this);
    let result = engine.call_fn_with_options::<Dynamic>(opts, &mut m.scope, &m.ast, f, args);
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

fn engine() -> Engine {
    let mut e = Engine::new();
    e.on_print(|s| log(s));
    e.on_debug(|s, _, _| log(s));

    // game objects
    e.register_type_with_name::<RObj>("Obj")
        .register_fn("static_get", |ty: &str, m: &str| RObj::new(Obj::static_get(ty, m)))
        .register_fn("static_set", |ty: &str, m: &str, v: Dynamic| Obj::static_set(ty, m, &to_obj(&v)))
        .register_fn("static_call", |ty: &str, m: &str, a: Array| {
            let a = args(&a);
            RObj::new(Obj::static_call(ty, m, &refs(&a)))
        })
        .register_fn("static_call", |ty: &str, m: &str| RObj::new(Obj::static_call(ty, m, &[])))
        .register_fn("get", |o: &mut RObj, m: &str| RObj::new(o.0.get(m)))
        .register_fn("set", |o: &mut RObj, m: &str, v: Dynamic| o.0.set(m, &to_obj(&v)))
        // `invoke`, not `call`: Rhai keeps `x.call(...)` for function pointers.
        .register_fn("invoke", |o: &mut RObj, m: &str, a: Array| {
            let a = args(&a);
            RObj::new(o.0.call(m, &refs(&a)))
        })
        .register_fn("invoke", |o: &mut RObj, m: &str| RObj::new(o.0.call(m, &[])))
        .register_fn("float", |o: &mut RObj| o.0.f64().map_or(Dynamic::UNIT, Dynamic::from))
        .register_fn("is_null", |o: &mut RObj| o.0.is_null())
        .register_fn("len", |o: &mut RObj| o.0.len().map_or(-1, |n| n as i64))
        .register_indexer_get(|o: &mut RObj, i: i64| RObj::new(o.0.index(i as usize)))
        .register_fn("to_string", |o: &mut RObj| o.0.to_string())
        .register_fn("to_debug", |o: &mut RObj| o.0.to_string());

    // patch("KSA.Vehicle", "SetEnum", "prefix" | "postfix", "my_fn") -> my_fn(instance, args) returns false to skip original
    e.register_fn("patch", |ty: &str, m: &str, kind: &str, f: &str| {
        let user = Box::leak(Box::new((CURRENT.get(), f.to_string()))) as *const _ as usize;
        oxidizer_api::patch(ty, m, kind == "postfix", hook_tramp, user)
    });

    // inside a hook: the hooked method's return value; set_result replaces it
    e.register_fn("result", || RObj::new(oxidizer_api::hook_result()))
        .register_fn("set_result", |v: Dynamic| oxidizer_api::set_hook_result(&to_obj(&v)));

    // keys, by the game's key names ("F5", "K", "Space"); pressed = this frame only
    e.register_fn("key_pressed", |k: &str| oxidizer_api::key_pressed(k))
        .register_fn("key_down", |k: &str| oxidizer_api::key_down(k));

    // settings saved per mod in <documents>/oxidizer/<mod>.toml: bools, numbers and strings
    e.register_fn("config_get", config_get).register_fn("config_set", config_set);

    // imgui (only inside gui())
    e.register_fn("begin", |t: &str| imgui::begin(t))
        .register_fn("end", imgui::end)
        .register_fn("text", |s: &str| imgui::text(s))
        .register_fn("button", |s: &str| imgui::button(s))
        .register_fn("checkbox", |l: &str, mut v: bool| {
            imgui::checkbox(l, &mut v);
            v
        })
        .register_fn("slider", |l: &str, v: f64, min: f64, max: f64| {
            let mut x = v as f32;
            imgui::slider_float(l, &mut x, min as f32, max as f32);
            x as f64
        })
        .register_fn("same_line", imgui::same_line)
        .register_fn("separator", imgui::separator)
        .register_fn("header", |l: &str| imgui::collapsing_header(l));

    // menu entries (only inside menu(), which runs inside the "Mods" menu)
    e.register_fn("menu_item", |l: &str| imgui::menu_item(l, false))
        .register_fn("menu_item", |l: &str, selected: bool| imgui::menu_item(l, selected))
        .register_fn("begin_menu", |l: &str| imgui::begin_menu(l))
        .register_fn("end_menu", imgui::end_menu);
    e
}
