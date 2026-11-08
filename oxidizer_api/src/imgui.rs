//! Direct calls into KSA's own ImGui library (its plain C exports, the ones the game itself calls), so mods share the game's ImGui context.
//! Only valid inside `Mod::gui` (and the menu functions inside `Mod::menu`).
//! Like Dear ImGui: always call [`end`] after [`begin`], and [`end_child`] after [`begin_child`].

use std::ffi::{CString, c_char, c_void};
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
    // (label, buffer, buffer size, [size,] flags, callback, user data)
    input_text: unsafe extern "C" fn(S, *mut u8, usize, i32, *const c_void, *mut c_void) -> bool,
    input_text_multiline: unsafe extern "C" fn(S, *mut u8, usize, *const ImVec2, i32, *const c_void, *mut c_void) -> bool,
    begin_tab_bar: unsafe extern "C" fn(S, i32) -> bool,
    end_tab_bar: unsafe extern "C" fn(),
    begin_tab_item: unsafe extern "C" fn(S, *mut bool, i32) -> bool,
    end_tab_item: unsafe extern "C" fn(),
    is_key_down: unsafe extern "C" fn(i32) -> bool,
    is_key_pressed: unsafe extern "C" fn(i32, bool) -> bool,
    set_keyboard_focus_here: unsafe extern "C" fn(i32),
}

fn fns() -> &'static Fns {
    static F: OnceLock<Fns> = OnceLock::new();
    F.get_or_init(|| unsafe {
        // Already loaded by the game, so this just returns the existing module.
        let lib = libloading::Library::new(libloading::library_filename("imgui")).expect("the game's ImGui library is not loaded");
        macro_rules! sym {
            ($n:literal) => {
                *lib.get(concat!($n, "\0").as_bytes()).expect($n)
            };
        }
        let f = Fns {
            begin: sym!("Begin"),
            end: sym!("End"),
            text: sym!("TextUnformatted"),
            button: sym!("Button"),
            checkbox: sym!("Checkbox"),
            slider_float: sym!("SliderFloat"),
            input_double: sym!("InputDouble"),
            same_line: sym!("SameLine"),
            separator: sym!("Separator"),
            collapsing_header: sym!("CollapsingHeader_0"),
            begin_menu: sym!("BeginMenu"),
            end_menu: sym!("EndMenu"),
            menu_item: sym!("MenuItem_0"),
            set_next_window_size: sym!("SetNextWindowSize"),
            begin_child: sym!("BeginChild_0"),
            end_child: sym!("EndChild"),
            scroll_y: sym!("GetScrollY"),
            scroll_max_y: sym!("GetScrollMaxY"),
            set_scroll_here_y: sym!("SetScrollHereY"),
            push_style_color: sym!("PushStyleColor_1"),
            pop_style_color: sym!("PopStyleColor"),
            input_text: sym!("InputText"),
            input_text_multiline: sym!("InputTextMultiline"),
            begin_tab_bar: sym!("BeginTabBar"),
            end_tab_bar: sym!("EndTabBar"),
            begin_tab_item: sym!("BeginTabItem"),
            end_tab_item: sym!("EndTabItem"),
            is_key_down: sym!("IsKeyDown"),
            is_key_pressed: sym!("IsKeyPressed"),
            set_keyboard_focus_here: sym!("SetKeyboardFocusHere"),
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

/// Flags for [`input_text`] and [`input_text_multiline`] (ImGuiInputTextFlags), combined with `|`.
pub mod text_flags {
    /// Tab types a tab character instead of moving to the next field.
    pub const ALLOW_TAB_INPUT: i32 = 32;
    /// The function returns true when Enter is pressed, instead of on every edit.
    pub const ENTER_RETURNS_TRUE: i32 = 64;
    pub const READ_ONLY: i32 = 512;
}

/// Flags for [`begin_tab_item`] (ImGuiTabItemFlags).
pub mod tab_flags {
    /// Shows a dot on the tab: its document has unsaved changes.
    pub const UNSAVED_DOCUMENT: i32 = 1;
    /// Makes this tab the selected one (this frame).
    pub const SET_SELECTED: i32 = 2;
}

/// Text that [`input_text`] and [`input_text_multiline`] edit in place.
///
/// It keeps 64 KiB of spare room past the text, because ImGui writes into the buffer as the player types: one
/// paste can add up to that much.
#[derive(Clone, Default)]
pub struct TextBuffer(Vec<u8>);

impl TextBuffer {
    const SPARE: usize = 64 * 1024;

    pub fn new(text: &str) -> TextBuffer {
        let mut b = TextBuffer::default();
        b.set(text);
        b
    }

    pub fn set(&mut self, text: &str) {
        self.0.clear();
        self.0.extend_from_slice(text.replace('\0', "").as_bytes());
        self.0.push(0);
    }

    pub fn as_str(&self) -> &str {
        let bytes = &self.0[..self.0.iter().position(|&b| b == 0).unwrap_or(self.0.len())];
        std::str::from_utf8(bytes).unwrap_or_else(|e| std::str::from_utf8(&bytes[..e.valid_up_to()]).unwrap())
    }

    /// The buffer for ImGui, with room to grow.
    fn room(&mut self) -> (*mut u8, usize) {
        let want = self.as_str().len() + Self::SPARE;
        if self.0.len() < want {
            self.0.resize(want, 0);
        }
        (self.0.as_mut_ptr(), self.0.len())
    }
}

/// A one-line text field. Returns true when the text was edited (or, with
/// [`text_flags::ENTER_RETURNS_TRUE`], when Enter was pressed).
pub fn input_text(label: &str, text: &mut TextBuffer, flags: i32) -> bool {
    let (p, n) = text.room();
    unsafe { (fns().input_text)(c(label).as_ptr(), p, n, flags, std::ptr::null(), std::ptr::null_mut()) }
}

/// A multi-line text box, `width` x `height` pixels: 0 is the default size, and a negative value fills the
/// window except that many pixels (so `-f32::MIN_POSITIVE` fills it all). Returns true when the text was edited.
pub fn input_text_multiline(label: &str, text: &mut TextBuffer, width: f32, height: f32, flags: i32) -> bool {
    let (p, n) = text.room();
    let size = ImVec2 { x: width, y: height };
    unsafe { (fns().input_text_multiline)(c(label).as_ptr(), p, n, &size, flags, std::ptr::null(), std::ptr::null_mut()) }
}

/// Tabs. Only call [`end_tab_bar`] when this returns true.
pub fn begin_tab_bar(id: &str) -> bool {
    unsafe { (fns().begin_tab_bar)(c(id).as_ptr(), 0) }
}

pub fn end_tab_bar() {
    unsafe { (fns().end_tab_bar)() }
}

/// A tab; true while it is the selected one. Only call [`end_tab_item`] when this returns true. With `open`, the
/// tab has a close button that sets it to false. Labels must be unique in the tab bar: add `##id` to make them so.
pub fn begin_tab_item(label: &str, open: Option<&mut bool>, flags: i32) -> bool {
    let open = open.map_or(std::ptr::null_mut(), |o| o as *mut bool);
    unsafe { (fns().begin_tab_item)(c(label).as_ptr(), open, flags) }
}

pub fn end_tab_item() {
    unsafe { (fns().end_tab_item)() }
}

/// True on the frame Ctrl (Cmd on a Mac keyboard layout ImGui maps to Ctrl) and the letter `key` went down. Unlike
/// [`crate::key_pressed`], this also sees keys typed into a text field, so it works for shortcuts like Ctrl+S.
pub fn ctrl_shortcut(key: char) -> bool {
    const KEY_A: i32 = 546; // ImGuiKey_A; the letters follow in order
    const MOD_CTRL: i32 = 1 << 12; // ImGuiMod_Ctrl
    let key = key.to_ascii_uppercase();
    if !key.is_ascii_uppercase() {
        return false;
    }
    unsafe { (fns().is_key_down)(MOD_CTRL) && (fns().is_key_pressed)(KEY_A + (key as i32 - 'A' as i32), false) }
}

/// Puts the keyboard focus on a widget: `0` the next one, `-1` the one just drawn (e.g. to keep typing in a
/// console line after Enter).
pub fn set_keyboard_focus_here(offset: i32) {
    unsafe { (fns().set_keyboard_focus_here)(offset) }
}
