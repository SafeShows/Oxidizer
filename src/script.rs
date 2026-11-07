//! Rhai script mods. Each script may define `init()`, `frame(t, dt)`, `gui(dt)`, `menu()`, `unload()`,
//! and `reload()`, which runs when the file was changed and reloaded while the game runs.
//! Per-mod state lives in `this` (an object map) and survives reloads,
//! e.g. `this.show = checkbox("Show", this.show ?? false);`.

use crate::{log, ui};
use oxidizer_api::{Handle, Obj, imgui};
use rhai::{Array, CallFnOptions, Dynamic, Engine, FuncArgs, Map, Scope, AST};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

const KIND: &str = "Rhai";
const RELOAD_CHECK: Duration = Duration::from_secs(1);

#[derive(Clone)]
struct RObj(Arc<Obj>);

impl RObj {
    fn new(o: Obj) -> RObj {
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

struct Scripts {
    engine: Engine,
    mods: Vec<Script>,
}

static SCRIPTS: Mutex<Option<Scripts>> = Mutex::new(None);
/// Index of the script currently running, so `patch()` knows who registered a hook.
static CURRENT: AtomicUsize = AtomicUsize::new(0);

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
        return;
    }
    let engine = engine();
    let mods = list
        .into_iter()
        .map(|(name, path)| {
            let modified = modified(&path);
            // A script that doesn't compile stays in the list, so fixing the file starts it.
            let (ast, scope, dead) = match compile(&engine, &path) {
                Ok((ast, scope)) => {
                    log(&format!("loaded script mod {name}"));
                    (ast, scope, false)
                }
                Err(e) => {
                    report(&name, e);
                    (AST::empty(), Scope::new(), true)
                }
            };
            Script { name, path, modified, ast, scope, this: Map::new().into(), dead, started: false }
        })
        .collect();
    let mut g = SCRIPTS.lock().unwrap();
    let s = g.insert(Scripts { engine, mods });
    for i in 0..s.mods.len() {
        start(s, i);
    }
}

/// Runs init() once per script.
fn start(s: &mut Scripts, i: usize) {
    if s.mods[i].dead {
        return;
    }
    call(s, i, "init", ());
    let m = &mut s.mods[i];
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
    let Ok(mut g) = SCRIPTS.try_lock() else { return };
    let Some(s) = g.as_mut() else { return };
    for i in 0..s.mods.len() {
        let t = modified(&s.mods[i].path);
        if t == s.mods[i].modified {
            continue;
        }
        s.mods[i].modified = t;
        match compile(&s.engine, &s.mods[i].path) {
            Ok((ast, scope)) => {
                let m = &mut s.mods[i];
                (m.ast, m.scope, m.dead) = (ast, scope, false);
                log(&format!("reloaded script mod {}", m.name));
                ui::set_status(&m.name, KIND, None);
                if m.started {
                    call(s, i, "reload", ());
                } else {
                    start(s, i);
                }
            }
            // The previous version keeps running (unless it had stopped).
            Err(e) => report(&s.mods[i].name.clone(), format!("reload failed: {e}")),
        }
    }
}

pub fn call_all(f: &str, args: impl FuncArgs + Clone) {
    // ponytail: skipped if a script is already running (re-entrant hook / other thread); per-mod locks if that bites.
    let Ok(mut g) = SCRIPTS.try_lock() else { return };
    let Some(s) = g.as_mut() else { return };
    for i in 0..s.mods.len() {
        call(s, i, f, args.clone());
    }
}

fn call(s: &mut Scripts, i: usize, f: &str, args: impl FuncArgs) -> Option<Dynamic> {
    let m = &mut s.mods[i];
    if m.dead || !m.ast.iter_functions().any(|x| x.name == f) {
        return None;
    }
    CURRENT.store(i, Ordering::Relaxed);
    let opts = CallFnOptions::new().eval_ast(false).bind_this_ptr(&mut m.this);
    match s.engine.call_fn_with_options::<Dynamic>(opts, &mut m.scope, &m.ast, f, args) {
        Ok(v) => Some(v),
        Err(e) => {
            // Errors in frame/gui would repeat every frame: stop the script until its file changes.
            m.dead = true;
            report(&m.name, format!("{f}: {e} (stopped until the file is saved again)"));
            None
        }
    }
}

extern "C" fn hook_tramp(user: usize, inst: Handle, args: Handle) -> u8 {
    let (inst, args) = unsafe { (RObj::new(Obj::from_handle(inst)), RObj::new(Obj::from_handle(args))) };
    let (i, f) = unsafe { &*(user as *const (usize, String)) };
    let Ok(mut g) = SCRIPTS.try_lock() else { return 1 };
    let Some(s) = g.as_mut() else { return 1 };
    call(s, *i, f, (inst, args)).and_then(|v| v.as_bool().ok()).unwrap_or(true) as u8
}

fn to_obj(d: &Dynamic) -> Arc<Obj> {
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
        .register_fn("call", |o: &mut RObj, m: &str, a: Array| {
            let a = args(&a);
            RObj::new(o.0.call(m, &refs(&a)))
        })
        .register_fn("call", |o: &mut RObj, m: &str| RObj::new(o.0.call(m, &[])))
        .register_fn("float", |o: &mut RObj| o.0.f64().map_or(Dynamic::UNIT, Dynamic::from))
        .register_fn("is_null", |o: &mut RObj| o.0.is_null())
        .register_fn("len", |o: &mut RObj| o.0.len().map_or(-1, |n| n as i64))
        .register_indexer_get(|o: &mut RObj, i: i64| RObj::new(o.0.index(i as usize)))
        .register_fn("to_string", |o: &mut RObj| o.0.to_string())
        .register_fn("to_debug", |o: &mut RObj| o.0.to_string());

    // patch("KSA.Vehicle", "SetEnum", "prefix" | "postfix", "my_fn") -> my_fn(instance, args) returns false to skip original
    e.register_fn("patch", |ty: &str, m: &str, kind: &str, f: &str| {
        let user = Box::leak(Box::new((CURRENT.load(Ordering::Relaxed), f.to_string()))) as *const _ as usize;
        oxidizer_api::patch(ty, m, kind == "postfix", hook_tramp, user)
    });

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
