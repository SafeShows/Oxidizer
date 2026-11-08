//! Typed access to the game for Rhai scripts, built at startup from the running game (bridge/Describe.cs), so it
//! always matches the installed version:
//!
//! ```rhai
//! let v = ksa::Program::controlled_vehicle();   // static members: functions in a module per namespace and type
//! print(v.id);                                  // instance properties and fields
//! ksa::Universe::set_simulation_speed(2.0, false); // overloads by argument count; enums: ksa::VehicleEngine::MainIgnite
//! ```
//!
//! Numbers, bools and strings come back as Rhai values, other objects as `Obj`. Static members are called by
//! metadata token (exact); instance members by name on the object's actual type, with the bridge picking the
//! overload. Definition files for editors with Rhai support are written to `<documents>/oxidizer/definitions/`.

use crate::script::{RObj, to_obj};
use crate::{internal, log};
use oxidizer_api::{Handle, Obj, api};
use rhai::plugin::RhaiFunc;
use rhai::{Dynamic, Engine, EvalAltResult, FuncRegistration, Module};
use std::any::TypeId;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// Names Rhai reserves, or Oxidizer's own `Obj` functions: generated names get a `_` instead of clashing.
const RESERVED: &[&str] = &[
    "as", "break", "call", "catch", "const", "continue", "curry", "debug", "do", "else", "eval", "export", "false",
    "float", "fn", "for", "get", "global", "if", "import", "in", "is", "is_def_fn", "is_def_var", "is_null",
    "is_shared", "len", "let", "loop", "print", "private", "return", "set", "switch", "this", "throw", "to_debug",
    "to_string", "true", "try", "type_of", "until", "while", "Fn",
];

fn ident(s: String) -> String {
    if RESERVED.contains(&s.as_str()) { s + "_" } else { s }
}

/// PascalCase to snake_case, with acronyms kept together (GetHTTPName -> get_http_name).
fn snake(name: &str) -> String {
    let c: Vec<char> = name.chars().collect();
    let mut s = String::new();
    for (i, &ch) in c.iter().enumerate() {
        if ch.is_uppercase() {
            let prev_lower = i > 0 && (c[i - 1].is_lowercase() || c[i - 1].is_ascii_digit());
            let acronym_end = i > 0 && c[i - 1].is_uppercase() && c.get(i + 1).is_some_and(|n| n.is_lowercase());
            if (prev_lower || acronym_end) && !s.ends_with('_') && !s.is_empty() {
                s.push('_');
            }
            s.extend(ch.to_lowercase());
        } else {
            s.push(if ch.is_alphanumeric() { ch } else { '_' });
        }
    }
    let s = s.trim_matches('_').to_string();
    ident(if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) { format!("_{s}") } else { s })
}

/// A game value as a Rhai value: (), bool, integer, float, string, or Obj.
pub fn to_dyn(o: Obj) -> Dynamic {
    match (internal().kind)(o.handle()) {
        0 => Dynamic::UNIT,
        1 => o.f64().is_some_and(|v| v != 0.0).into(),
        2 => o.i64().unwrap_or(0).into(),
        3 => o.f64().unwrap_or(f64::NAN).into(),
        4 => o.to_string().into(),
        _ => Dynamic::from(RObj::new(o)),
    }
}

fn handles(args: &[Arc<Obj>]) -> Vec<Handle> {
    args.iter().map(|a| a.handle()).collect()
}

fn invoke(token: i32, args: &[&mut Dynamic]) -> Dynamic {
    let objs: Vec<Arc<Obj>> = args.iter().map(|a| to_obj(a)).collect();
    let hs = handles(&objs);
    to_dyn(unsafe { Obj::from_handle((api().invoke)(0, token, hs.as_ptr(), hs.len())) })
}

/// A function whose parameters are only known at runtime (every argument is a Dynamic).
fn raw(f: impl Fn(&mut [&mut Dynamic]) -> Dynamic + Send + Sync + 'static) -> RhaiFunc {
    RhaiFunc::Pure {
        func: Arc::new(move |_, args| -> Result<Dynamic, Box<EvalAltResult>> { Ok(f(args)) }),
        has_context: false,
        is_pure: true,
        is_volatile: true,
    }
}

/// Same, with the first argument an Obj passed by reference (a method or property of a game object).
fn method(f: impl Fn(&RObj, &mut [&mut Dynamic]) -> Dynamic + Send + Sync + 'static) -> RhaiFunc {
    RhaiFunc::Method {
        func: Arc::new(move |_, args| -> Result<Dynamic, Box<EvalAltResult>> {
            let (this, rest) = args.split_first_mut().unwrap();
            let this = this.read_lock::<RObj>().map(|o| o.clone());
            Ok(match this {
                Some(o) => f(&o, rest),
                None => Dynamic::UNIT,
            })
        }),
        has_context: false,
        is_pure: true,
        is_volatile: true,
    }
}

fn any_types(n: usize) -> Vec<TypeId> {
    vec![TypeId::of::<Dynamic>(); n]
}

fn obj_and(n: usize) -> Vec<TypeId> {
    std::iter::once(TypeId::of::<RObj>()).chain(any_types(n)).collect()
}

/// "speed:Double,alert:Boolean" -> ["speed: Double", "alert: Boolean"] (plus the return type) for definitions.
fn params_info(params: &str, arity: usize, returns: &str) -> Vec<String> {
    let mut info: Vec<String> = if params.is_empty() {
        (1..=arity).map(|i| format!("arg{i}: ?")).collect()
    } else {
        params.split(',').map(|p| p.replacen(':', ": ", 1)).collect()
    };
    info.push(if returns.is_empty() || returns == "Void" { "()".into() } else { returns.into() });
    info
}

#[derive(Default)]
struct Node {
    module: Module,
    children: BTreeMap<String, Node>,
}

impl Node {
    fn at(&mut self, path: &[String]) -> &mut Node {
        path.iter().fold(self, |n, seg| n.children.entry(seg.clone()).or_default())
    }

    fn build(mut self) -> Module {
        for (name, child) in self.children {
            self.module.set_sub_module(name, child.build());
        }
        self.module
    }
}

/// "KSA.Rendering.Foo+Bar" -> (["ksa", "rendering"], "Foo_Bar")
fn type_path(full: &str) -> (Vec<String>, String) {
    let (outer, nested) = full.split_once('+').map_or((full, None), |(o, n)| (o, Some(n)));
    let (ns, name) = outer.rsplit_once('.').unwrap_or(("", outer));
    let name = match nested {
        Some(n) => format!("{name}_{}", n.replace('+', "_")),
        None => name.to_string(),
    };
    (ns.split('.').filter(|s| !s.is_empty()).map(snake).collect(), name)
}

/// Registers the game's types into `engine`. Returns how many functions were added.
pub fn register(engine: &mut Engine) -> usize {
    let text = oxidizer_api::read_string(|b, c| (internal().describe)(b, c));
    let mut root = Node::default();
    let mut instance = Module::new();
    let mut used: HashMap<(String, String, usize), ()> = HashMap::new(); // (type path, name, arity) taken
    let mut props: HashSet<String> = HashSet::new();
    let mut methods: HashSet<(String, usize)> = HashSet::new();
    let mut count = 0;

    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f.as_slice() {
            ["E", ty, name, value] => {
                let (ns, tname) = type_path(ty);
                let node = root.at(&[ns, vec![tname]].concat());
                node.module.set_var(*name, value.parse::<i64>().unwrap_or(0));
            }
            ["S", ty, kind, name, arity, token, setter, params, returns] => {
                let (ns, tname) = type_path(ty);
                let path = [ns, vec![tname.clone()]].concat();
                let module = &mut root.at(&path).module;
                let arity: usize = arity.parse().unwrap_or(0);
                let token: i32 = token.parse().unwrap_or(0);
                let setter: i32 = setter.parse().unwrap_or(0);
                let mut add = |base: String, n: usize, func: RhaiFunc, info: Vec<String>, doc: String| {
                    let key = path.join("::");
                    let mut fname = base.clone();
                    for i in 2.. {
                        if used.insert((key.clone(), fname.clone(), n), ()).is_none() {
                            break;
                        }
                        fname = format!("{base}_{i}");
                    }
                    FuncRegistration::new(fname)
                        .with_params_info(info)
                        .with_comments([format!("/// {doc}")])
                        .set_into_module_raw(module, any_types(n), func);
                    count += 1;
                };
                let member = format!("{ty}.{name}");
                match *kind {
                    "m" | "c" => add(
                        if *kind == "c" { "new".into() } else { snake(name) },
                        arity,
                        raw(move |args| invoke(token, args)),
                        params_info(params, arity, returns),
                        format!("`{member}`"),
                    ),
                    "p" => {
                        add(snake(name), 0, raw(move |args| invoke(token, args)), params_info("", 0, returns), format!("`{member}` (static property)"));
                        if setter != 0 {
                            add(format!("set_{}", snake(name)), 1, raw(move |args| invoke(setter, args)), params_info(&format!("value:{returns}"), 1, ""), format!("sets `{member}`"));
                        }
                    }
                    "f" => {
                        add(snake(name), 0, raw(move |_| to_dyn(unsafe { Obj::from_handle((api().field_get)(0, token)) })), params_info("", 0, returns), format!("`{member}` (static field)"));
                        if setter != 0 {
                            add(
                                format!("set_{}", snake(name)),
                                1,
                                raw(move |args| {
                                    (api().field_set)(0, token, to_obj(&args[0]).handle());
                                    Dynamic::UNIT
                                }),
                                params_info(&format!("value:{returns}"), 1, ""),
                                format!("sets `{member}`"),
                            );
                        }
                    }
                    _ => {}
                }
            }
            ["I", kind, name, arity, writable, params, returns, owners] => {
                let arity: usize = arity.parse().unwrap_or(0);
                let rname = snake(name);
                let cs = name.to_string();
                // Types are only known when every game type with this member agrees; "?" otherwise.
                let ret = if returns.is_empty() { "?" } else { returns };
                let on = match owners.split_once(",+") {
                    Some((names, more)) => format!("on {} and {more} more", names.replace(',', ", ")),
                    None => format!("on {}", owners.replace(',', ", ")),
                };
                if *kind == "m" {
                    if !methods.insert((rname.clone(), arity)) {
                        continue;
                    }
                    FuncRegistration::new(rname)
                        .in_global_namespace()
                        .with_params_info(std::iter::once("obj: Obj".to_string()).chain(params_info(params, arity, ret)))
                        .with_comments([format!("/// the game method `{cs}` {on}")])
                        .set_into_module_raw(
                            &mut instance,
                            obj_and(arity),
                            method(move |o, args| {
                                let objs: Vec<Arc<Obj>> = args.iter().map(|a| to_obj(a)).collect();
                                let refs: Vec<&Obj> = objs.iter().map(|a| &**a).collect();
                                to_dyn(o.0.call(&cs, &refs))
                            }),
                        );
                    count += 1;
                } else {
                    if !props.insert(rname.clone()) {
                        continue;
                    }
                    let get_cs = cs.clone();
                    FuncRegistration::new_getter(&rname)
                        .with_params_info(["obj: Obj", ret])
                        .with_comments([format!("/// the game property or field `{cs}` {on}")])
                        .set_into_module_raw(&mut instance, obj_and(0), method(move |o, _| to_dyn(o.0.get(&get_cs))));
                    count += 1;
                    if *writable == "1" {
                        FuncRegistration::new_setter(&rname).with_params_info(["obj: Obj", &format!("value: {ret}"), "()"]).set_into_module_raw(
                            &mut instance,
                            obj_and(1),
                            method(move |o, args| {
                                let v = to_obj(&args[0]);
                                // After a method call on a property (`v.parent.get(..)`), Rhai writes the property
                                // back, even read-only ones: one setter serves every type with a member of this
                                // name. Writing back an unchanged value is skipped.
                                let cur = o.0.get(&cs);
                                let same = if cur.is_null() { v.is_null() } else { cur.call("Equals", &[&*v]).f64() == Some(1.0) };
                                if !same {
                                    o.0.set(&cs, &v);
                                }
                                Dynamic::UNIT
                            }),
                        );
                        count += 1;
                    }
                }
            }
            _ => {}
        }
    }

    engine.register_global_module(instance.into());
    for (name, node) in root.children {
        engine.register_static_module(name, node.build().into());
    }
    count
}

/// Writes definition files for editors, once per game build (`<dir>/version.txt` remembers which).
pub fn write_definitions(engine: &Engine, dir: &Path) {
    let version = oxidizer_api::read_string(|b, c| (api().ksa_version)(b, c));
    let marker = dir.join("version.txt");
    if std::fs::read_to_string(&marker).is_ok_and(|v| v == version) {
        return;
    }
    let _ = std::fs::remove_dir_all(dir);
    match engine.definitions().write_to_dir(dir) {
        Ok(()) => {
            let _ = std::fs::write(&marker, &version);
            log(&format!("wrote Rhai definitions for editors to {}", dir.display()));
        }
        Err(e) => log(&format!("writing Rhai definitions to {}: {e}", dir.display())),
    }
}
