//! Direct calls into KSA's own imgui.dll (C++ exports, same ImGui context as the game).
//! Only valid inside `Mod::gui`. Like Dear ImGui: always call [`end`] after [`begin`].

use std::ffi::{CString, c_char};
use std::sync::OnceLock;

#[repr(C)]
struct ImVec2 {
    x: f32,
    y: f32,
}

type S = *const c_char;

struct Fns {
    begin: unsafe extern "C" fn(S, *mut bool, i32) -> bool,
    end: unsafe extern "C" fn(),
    text: unsafe extern "C" fn(S, S),
    button: unsafe extern "C" fn(S, *const ImVec2) -> bool,
    checkbox: unsafe extern "C" fn(S, *mut bool) -> bool,
    slider_float: unsafe extern "C" fn(S, *mut f32, f32, f32, S, i32) -> bool,
    input_double: unsafe extern "C" fn(S, *mut f64, f64, f64, S, i32) -> bool,
    same_line: unsafe extern "C" fn(f32, f32),
    separator: unsafe extern "C" fn(),
    collapsing_header: unsafe extern "C" fn(S, i32) -> bool,
}

fn fns() -> &'static Fns {
    static F: OnceLock<Fns> = OnceLock::new();
    F.get_or_init(|| unsafe {
        // Already loaded by the game, so this just returns the existing module.
        let lib = libloading::Library::new("imgui.dll").expect("imgui.dll not loaded");
        macro_rules! sym {
            ($n:literal) => {
                *lib.get(concat!($n, "\0").as_bytes()).expect($n)
            };
        }
        let f = Fns {
            begin: sym!("?Begin@ImGui@@YA_NPEBDPEA_NH@Z"),
            end: sym!("?End@ImGui@@YAXXZ"),
            text: sym!("?TextUnformatted@ImGui@@YAXPEBD0@Z"),
            button: sym!("?Button@ImGui@@YA_NPEBDAEBUImVec2@@@Z"),
            checkbox: sym!("?Checkbox@ImGui@@YA_NPEBDPEA_N@Z"),
            slider_float: sym!("?SliderFloat@ImGui@@YA_NPEBDPEAMMM0H@Z"),
            input_double: sym!("?InputDouble@ImGui@@YA_NPEBDPEANNN0H@Z"),
            same_line: sym!("?SameLine@ImGui@@YAXMM@Z"),
            separator: sym!("?Separator@ImGui@@YAXXZ"),
            collapsing_header: sym!("?CollapsingHeader@ImGui@@YA_NPEBDH@Z"),
        };
        std::mem::forget(lib);
        f
    })
}

fn c(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap()
}

/// Returns false when collapsed. Call [`end`] regardless.
pub fn begin(title: &str) -> bool {
    unsafe { (fns().begin)(c(title).as_ptr(), std::ptr::null_mut(), 0) }
}

pub fn end() {
    unsafe { (fns().end)() }
}

pub fn text(s: &str) {
    let p = s.as_ptr() as S;
    unsafe { (fns().text)(p, p.add(s.len())) }
}

pub fn button(label: &str) -> bool {
    unsafe { (fns().button)(c(label).as_ptr(), &ImVec2 { x: 0.0, y: 0.0 }) }
}

pub fn checkbox(label: &str, v: &mut bool) -> bool {
    unsafe { (fns().checkbox)(c(label).as_ptr(), v) }
}

pub fn slider_float(label: &str, v: &mut f32, min: f32, max: f32) -> bool {
    unsafe { (fns().slider_float)(c(label).as_ptr(), v, min, max, c"%.3f".as_ptr(), 0) }
}

pub fn input_double(label: &str, v: &mut f64) -> bool {
    unsafe { (fns().input_double)(c(label).as_ptr(), v, 0.0, 0.0, c"%.6f".as_ptr(), 0) }
}

pub fn same_line() {
    unsafe { (fns().same_line)(0.0, -1.0) }
}

pub fn separator() {
    unsafe { (fns().separator)() }
}

pub fn collapsing_header(label: &str) -> bool {
    unsafe { (fns().collapsing_header)(c(label).as_ptr(), 0) }
}
