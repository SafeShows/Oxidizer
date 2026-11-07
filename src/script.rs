//! Rhai script mods. Each script may define `init()`, `frame(t, dt)`, `gui(dt)`, `unload()`.
//! Per-mod state lives in `this` (an object map), e.g. `this.show = checkbox("Show", this.show ?? false);`.

use crate::log;
use oxidizer_api::{Handle, Obj, imgui};
use rhai::{Array, CallFnOptions, Dynamic, Engine, FuncArgs, Map, Scope, AST};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct RObj(Arc<Obj>);

impl RObj {
    fn new(o: Obj) -> RObj {
        RObj(Arc::new(o))
    }
}

struct Script {
    name: String,
    ast: AST,
    scope: Scope<'static>,
    this: Dynamic,
    dead: bool,
}

struct Scripts {
    engine: Engine,
    mods: Vec<Script>,
}

static SCRIPTS: Mutex<Option<Scripts>> = Mutex::new(None);
/// Index of the script currently running, so `patch()` knows who registered a hook.
static CURRENT: AtomicUsize = AtomicUsize::new(0);

pub fn load(list: Vec<(String, PathBuf)>) {
    if list.is_empty() {
        return;
    }
    let engine = engine();
    let mut mods = vec![];
    for (name, path) in list {
        let mut scope = Scope::new();
        match engine.compile_file(path).and_then(|ast| engine.run_ast_with_scope(&mut scope, &ast).map(|_| ast)) {
            Ok(ast) => {
                log(&format!("loaded script mod {name}"));
                mods.push(Script { name, ast, scope, this: Map::new().into(), dead: false });
            }
            Err(e) => log(&format!("{name}: {e}")),
        }
    }
    *SCRIPTS.lock().unwrap() = Some(Scripts { engine, mods });
    call_all("init", ());
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
            // Errors in frame/gui would repeat every frame: stop the mod instead.
            log(&format!("{}: {f}: {e} (script disabled)", m.name));
            m.dead = true;
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
    e
}
