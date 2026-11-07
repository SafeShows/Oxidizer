//! Direct calls into KSA's own imgui.dll (C++ exports, same ImGui context as the game).
//! Only valid inside `Mod::gui` (and the menu functions inside `Mod::menu`).
//! Like Dear ImGui: always call [`end`] after [`begin`], and [`end_child`] after [`begin_child`].

use std::ffi::{CString, c_char};
use std::sync::OnceLock;

#[repr(C)]
struct ImVec2 {
    x: f32,
    y: f32,
}

#[repr(C)]
struct ImVec4 {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

const COL_TEXT: i32 = 0; // ImGuiCol_Text
const COND_FIRST_USE_EVER: i32 = 4;
const CHILD_BORDERS: i32 = 1;

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
    begin_menu: unsafe extern "C" fn(S, bool) -> bool,
    end_menu: unsafe extern "C" fn(),
    menu_item: unsafe extern "C" fn(S, S, bool, bool) -> bool,
    set_next_window_size: unsafe extern "C" fn(*const ImVec2, i32),
    begin_child: unsafe extern "C" fn(S, *const ImVec2, i32, i32) -> bool,
    end_child: unsafe extern "C" fn(),
    scroll_y: unsafe extern "C" fn() -> f32,
    scroll_max_y: unsafe extern "C" fn() -> f32,
    set_scroll_here_y: unsafe extern "C" fn(f32),
    push_style_color: unsafe extern "C" fn(i32, *const ImVec4),
    pop_style_color: unsafe extern "C" fn(i32),
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
            begin_menu: sym!("?BeginMenu@ImGui@@YA_NPEBD_N@Z"),
            end_menu: sym!("?EndMenu@ImGui@@YAXXZ"),
            menu_item: sym!("?MenuItem@ImGui@@YA_NPEBD0_N1@Z"),
            set_next_window_size: sym!("?SetNextWindowSize@ImGui@@YAXAEBUImVec2@@H@Z"),
            begin_child: sym!("?BeginChild@ImGui@@YA_NPEBDAEBUImVec2@@HH@Z"),
            end_child: sym!("?EndChild@ImGui@@YAXXZ"),
            scroll_y: sym!("?GetScrollY@ImGui@@YAMXZ"),
            scroll_max_y: sym!("?GetScrollMaxY@ImGui@@YAMXZ"),
            set_scroll_here_y: sym!("?SetScrollHereY@ImGui@@YAXM@Z"),
            push_style_color: sym!("?PushStyleColor@ImGui@@YAXHAEBUImVec4@@@Z"),
            pop_style_color: sym!("?PopStyleColor@ImGui@@YAXH@Z"),
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

/// A window with a close button; `open` is set to false when it is clicked. Call [`end`] regardless.
pub fn begin_closable(title: &str, open: &mut bool) -> bool {
    unsafe { (fns().begin)(c(title).as_ptr(), open, 0) }
}

/// Size of the next window the first time it is ever shown (later the player's size is kept).
pub fn set_next_window_size(w: f32, h: f32) {
    unsafe { (fns().set_next_window_size)(&ImVec2 { x: w, y: h }, COND_FIRST_USE_EVER) }
}

/// A submenu. Only call [`end_menu`] when this returns true.
pub fn begin_menu(label: &str) -> bool {
    unsafe { (fns().begin_menu)(c(label).as_ptr(), true) }
}

pub fn end_menu() {
    unsafe { (fns().end_menu)() }
}

/// A menu entry, shown with a check mark when `selected`. Returns true when clicked.
pub fn menu_item(label: &str, selected: bool) -> bool {
    unsafe { (fns().menu_item)(c(label).as_ptr(), std::ptr::null(), selected, true) }
}

/// A scrolling region with a border; a height of 0 fills the rest of the window. Call [`end_child`] regardless.
pub fn begin_child(id: &str, height: f32) -> bool {
    unsafe { (fns().begin_child)(c(id).as_ptr(), &ImVec2 { x: 0.0, y: height }, CHILD_BORDERS, 0) }
}

pub fn end_child() {
    unsafe { (fns().end_child)() }
}

/// Keeps the current window scrolled to the bottom unless the player scrolled up. Call after the content.
pub fn stick_to_bottom() {
    unsafe {
        if (fns().scroll_y)() >= (fns().scroll_max_y)() {
            (fns().set_scroll_here_y)(1.0)
        }
    }
}

/// Text in a color (RGBA, 0..1).
pub fn text_colored(rgba: [f32; 4], s: &str) {
    let [x, y, z, w] = rgba;
    unsafe { (fns().push_style_color)(COL_TEXT, &ImVec4 { x, y, z, w }) }
    text(s);
    unsafe { (fns().pop_style_color)(1) }
}
