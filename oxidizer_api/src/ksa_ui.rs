//! KSA's own UI kit, the one the game's windows use: `KSA.ConsoleStyle` windows (title bar with a signature, body,
//! footer) and `KSA.ConsoleWidgets` controls (styled buttons, labelled rows, sliders, readouts...). Mods drawn with
//! it look like the rest of the game; plain [`crate::imgui`] widgets look like stock ImGui.
//!
//! Only inside `Mod::gui`. A window goes:
//!
//! ```ignore
//! if ksa_ui::begin_window("my_mod", "My mod", "MOD", Some(&mut self.open), 420.0, 300.0, 0) {
//!     ksa_ui::begin_body();
//!     ksa_ui::push_widget_style();
//!     ksa_ui::begin_row("Enabled");
//!     ksa_ui::checkbox("enabled", &mut self.enabled);
//!     ksa_ui::end_row();
//!     ksa_ui::pop_widget_style();
//!     ksa_ui::end_body();
//!     ksa_ui::begin_footer();
//!     ksa_ui::footer_status("Ready");
//!     ksa_ui::end_footer();
//!     ksa_ui::end_window();
//! } // when begin_window returns false the window is already ended: don't call end_window
//! ```
//!
//! Needs API version 8 (Oxidizer 0.15). With an older bridge every call does nothing and returns false.

use crate::api;

fn call(op: i32, strs: &[&str], f: &mut [f32], i: &mut [i32], b: &mut [u8], out: Option<&mut Vec<u8>>) -> u8 {
    let pairs: Vec<usize> = strs.iter().flat_map(|s| [s.as_ptr() as usize, s.len()]).collect();
    let mut out_len = 0usize;
    let (buf, cap) = match out {
        Some(v) => (v.as_mut_ptr(), v.len()),
        None => (std::ptr::null_mut(), 0),
    };
    let r = (api().ksa_ui)(op, pairs.as_ptr(), strs.len(), f.as_mut_ptr(), i.as_mut_ptr(), b.as_mut_ptr(), buf, cap, &mut out_len);
    LAST_LEN.with(|l| l.set(out_len));
    r
}

thread_local!(static LAST_LEN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) });

fn void(op: i32, strs: &[&str]) {
    call(op, strs, &mut [0.0], &mut [0], &mut [0, 0], None);
}

fn yes(op: i32, strs: &[&str]) -> bool {
    call(op, strs, &mut [0.0], &mut [0], &mut [0, 0], None) != 0
}

/// A KSA window: `id` keeps its place and size, `title` is shown in the title bar with `signature` (a short tag,
/// like the game's "PKSA") beside it, `width` x `height` is its size the first time it appears, `flags` are
/// [`crate::imgui::window_flags`]. With `open`, the title bar has a close button that sets it to false.
///
/// When this returns false the window is collapsed or hidden and already ended: only call [`end_window`] (after
/// [`end_body`] and the footer) when it returns true.
pub fn begin_window(id: &str, title: &str, signature: &str, open: Option<&mut bool>, width: f32, height: f32, flags: i32) -> bool {
    let mut size = [width, height];
    let mut fl = [flags];
    match open {
        Some(o) => {
            let mut b = [*o as u8, 0];
            let r = call(0, &[id, title, signature], &mut size, &mut fl, &mut b, None) != 0;
            *o = b[0] != 0;
            r
        }
        None => call(1, &[id, title, signature], &mut size, &mut fl, &mut [0, 0], None) != 0,
    }
}

pub fn end_window() {
    void(2, &[]);
}

/// The window's scrolling content area, between the title bar and the footer.
pub fn begin_body() {
    void(3, &[]);
}

pub fn end_body() {
    void(4, &[]);
}

/// The window's bottom bar (status text, buttons).
pub fn begin_footer() {
    void(5, &[]);
}

pub fn end_footer() {
    void(6, &[]);
}

/// Status text in the footer.
pub fn footer_status(text: &str) {
    void(7, &[text]);
}

/// KSA's spacing and colors for plain ImGui widgets in the body; pair with [`pop_widget_style`].
pub fn push_widget_style() {
    void(8, &[]);
}

pub fn pop_widget_style() {
    void(9, &[]);
}

/// A button; true when clicked.
pub fn button(label: &str) -> bool {
    yes(10, &[label])
}

/// The main action's button (highlighted).
pub fn primary_button(label: &str) -> bool {
    yes(11, &[label])
}

/// A green button (start, enable, confirm).
pub fn positive_button(label: &str) -> bool {
    yes(12, &[label])
}

/// A red button (stop, delete).
pub fn danger_button(label: &str) -> bool {
    yes(13, &[label])
}

/// A section title with a rule under it.
pub fn region_header(label: &str) {
    void(14, &[label]);
}

/// A horizontal rule.
pub fn rule() {
    void(15, &[]);
}

/// A "key  value" line, the value aligned in a column.
pub fn readout(key: &str, value: &str) {
    void(16, &[key, value]);
}

/// Text in the value style.
pub fn value_text(text: &str) {
    void(17, &[text]);
}

/// A labelled row: the label on the left, the next control on the right. Pair with [`end_row`].
pub fn begin_row(label: &str) {
    void(18, &[label]);
}

pub fn end_row() {
    void(19, &[]);
}

/// A checkbox (put it in a row for a label); true when it changed.
pub fn checkbox(id: &str, value: &mut bool) -> bool {
    let mut b = [*value as u8, 0];
    let r = call(20, &[id], &mut [0.0], &mut [0], &mut b, None) != 0;
    *value = b[0] != 0;
    r
}

/// A slider showing `value_text` (e.g. `format!("{v:.2}")`); true when it changed.
pub fn slider_float(id: &str, value: &mut f32, min: f32, max: f32, value_text: &str) -> bool {
    let mut f = [*value, min, max];
    let r = call(21, &[id, value_text], &mut f, &mut [0], &mut [0, 0], None) != 0;
    *value = f[0];
    r
}

pub fn slider_int(id: &str, value: &mut i32, min: i32, max: i32, value_text: &str) -> bool {
    let mut i = [*value, min, max];
    let r = call(22, &[id, value_text], &mut [0.0], &mut i, &mut [0, 0], None) != 0;
    *value = i[0];
    r
}

/// A number dragged with the mouse, showing `value_text`; true when it changed.
pub fn drag_float(id: &str, value: &mut f32, min: f32, max: f32, value_text: &str) -> bool {
    let mut f = [*value, min, max];
    let r = call(23, &[id, value_text], &mut f, &mut [0], &mut [0, 0], None) != 0;
    *value = f[0];
    r
}

pub fn drag_int(id: &str, value: &mut i32, min: i32, max: i32, value_text: &str) -> bool {
    let mut i = [*value, min, max];
    let r = call(24, &[id, value_text], &mut [0.0], &mut i, &mut [0, 0], None) != 0;
    *value = i[0];
    r
}

/// A drop-down showing `preview`; true while open: add [`crate::imgui::selectable`]s, then [`end_combo`].
pub fn begin_combo(id: &str, preview: &str) -> bool {
    yes(25, &[id, preview])
}

pub fn end_combo() {
    void(26, &[]);
}

/// A drop-down of `items`; `current` is the chosen index. True when it changed.
pub fn combo(id: &str, current: &mut i32, items: &[&str]) -> bool {
    let preview = items.get(*current as usize).copied().unwrap_or("");
    let mut changed = false;
    if begin_combo(id, preview) {
        for (k, item) in items.iter().enumerate() {
            if crate::imgui::selectable(item, k as i32 == *current) && k as i32 != *current {
                *current = k as i32;
                changed = true;
            }
        }
        end_combo();
    }
    changed
}

/// A one-line text field; true when the text changed.
pub fn text_line(id: &str, text: &mut String) -> bool {
    let mut buf = vec![0u8; text.len() + 64 * 1024];
    let r = call(27, &[id, text.as_str()], &mut [0.0], &mut [0], &mut [0, 0], Some(&mut buf));
    let n = LAST_LEN.with(|l| l.get());
    if n > 0 && n <= buf.len() {
        *text = String::from_utf8_lossy(&buf[..n]).into_owned();
    }
    r != 0
}

/// A color swatch with RGB editing (0..1); true when it changed.
pub fn color_edit(id: &str, rgb: &mut [f32; 3]) -> bool {
    call(28, &[id], rgb, &mut [0], &mut [0, 0], None) != 0
}

/// A row of joined toggle buttons; returns the chosen index.
pub fn segmented(id: &str, items: &[&str], active: i32) -> i32 {
    let joined = items.join("\n");
    let mut i = [active];
    call(29, &[id, &joined], &mut [0.0], &mut i, &mut [0, 0], None);
    i[0]
}

/// KSA's tooltip for the widget just drawn, while the mouse rests on it.
pub fn item_tooltip(text: &str) {
    void(30, &[text]);
}
