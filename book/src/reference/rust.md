# Rust API reference

Everything in the [`oxidizer_api`](https://crates.io/crates/oxidizer_api) crate, version 0.5 (Oxidizer 0.8 and later; `Mod::loaded` is new in 0.5, the rest is in 0.4 and Oxidizer 0.7). The same items, with their doc comments, are on [docs.rs](https://docs.rs/oxidizer_api).

## The mod

| Item | |
|---|---|
| `trait Mod: Send + 'static` | Your mod. `fn init() -> Self` is required (runs before the game's `Main`); `fn loaded(&mut self)` (the game has loaded its content and mods), `fn frame(&mut self, player_time: f64, dt: f64)`, `fn gui(&mut self, dt: f64)`, `fn menu(&mut self)`, `fn unload(&mut self)` are optional. See [The Mod trait](../rust/mod-trait.md). |
| `oxidizer_mod!(Type)` | Registers `Type` as the library's mod: exports the functions Oxidizer calls and catches panics in them. Once per library. |
| `const API_VERSION: u32` | The API version the crate is built for (4 in 0.5). Oxidizer refuses mods built for a newer version than it has. |

## Game objects: `Obj`

`struct Obj` is a game object held by Oxidizer, released on drop.

| Method | |
|---|---|
| `Obj::static_get(ty: &str, member: &str) -> Obj` | Static property or field. `ty` like `"KSA.Program, KSA"`. |
| `Obj::static_set(ty: &str, member: &str, value: &Obj) -> bool` | Sets one; true on success. |
| `Obj::static_call(ty: &str, method: &str, args: &[&Obj]) -> Obj` | Static method. |
| `obj.get(member: &str) -> Obj` | Property or field. |
| `obj.set(member: &str, value: &Obj) -> bool` | Sets one. |
| `obj.call(method: &str, args: &[&Obj]) -> Obj` | Method. A name may carry parameter types to pick an overload: `"SetSimulationSpeed(double, bool)"`. |
| `obj.f64() -> Option<f64>` | The number (also bools and enums). |
| `obj.i64() -> Option<i64>` | The integer or enum value, exact for large values. |
| `obj.len() -> Option<usize>` | Length of an array or collection. |
| `obj.index(i: usize) -> Obj` | Element `i`. |
| `obj.is_null() -> bool` | Whether it is null. |
| `obj.to_string()` / `Display` | The object's `ToString()`. |
| `Obj::NULL` | A null object. |
| `Obj::from(f64 \| i64 \| bool \| &str)` | A game value, for arguments and `set`. |
| `obj.handle() -> Handle`, `unsafe Obj::from_handle(h: Handle) -> Obj` | The raw handle, for the low-level API. |

## Hooks

| Function | |
|---|---|
| `hook(ty: &str, method: &str, postfix: bool, f: fn(&Obj, &Obj) -> bool) -> bool` | Runs `f(instance, args)` before (`postfix = false`) or after the game's method. A prefix returning false skips the method. Panics in `f` are caught. True when the patch is in place. |
| `hook_result() -> Obj` | Inside a hook: the method's return value. |
| `set_hook_result(value: &Obj) -> bool` | Inside a hook: replaces it. |
| `patch(ty: &str, method: &str, postfix: bool, cb: HookFn, user: usize) -> bool` | The raw form: `HookFn = extern "C" fn(user: usize, instance: Handle, args: Handle) -> u8` owns both handles and returns 0 to skip the method. |

## Keys, settings, files

| Function | |
|---|---|
| `key_pressed(key: &str) -> bool` | True on the frame the key went down. Game (GLFW) key names: `"F5"`, `"K"`, `"Number1"`, `"Space"`, `"LeftShift"`, ... |
| `key_down(key: &str) -> bool` | True while held. |
| `config_path(mod_id: &str) -> PathBuf` | `<game documents>/oxidizer/<mod_id>.toml`, for your settings (its folder is created). |
| `mods_dir() -> PathBuf` | The game's mods folder. |

## Log, mod states, Rhai

| Function | |
|---|---|
| `log(s: &str)` | Writes to Oxidizer's log. |
| `log_next() -> u64` | The number the next log line will get. |
| `log_lines(from: u64) -> Vec<String>` | The log's lines from number `from` on, of the last few hundred kept. |
| `mod_statuses() -> Vec<ModStatus>` | Every Oxidizer mod. `ModStatus { id: String, kind: String /* "Rust" or "Rhai" */, problem: Option<String> }`; `problem` is `None` while it runs. |
| `rhai_eval(code: &str) -> Result<String, String>` | Runs Rhai in Oxidizer's script engine with everything Rhai mods can use; the value as text (empty for `()`), or the error. Variables persist between calls. |

## `imgui`

The game's own ImGui. Only valid inside `Mod::gui` (menu functions inside `Mod::menu`). See [Windows and menus](../rust/ui.md).

| Function | |
|---|---|
| `begin(title: &str) -> bool` | Starts a window (false while collapsed); call `end()` regardless. |
| `begin_closable(title: &str, open: &mut bool) -> bool` | A window with a close button that sets `open` to false. |
| `end()` | Ends the window. |
| `set_next_window_size(w: f32, h: f32)` | The next window's size the first time it is shown. |
| `text(s: &str)` | Text. |
| `text_colored(rgba: [f32; 4], s: &str)` | Text in a color (0..1 per channel). |
| `button(label: &str) -> bool` | True when clicked. |
| `checkbox(label: &str, v: &mut bool) -> bool` | True when changed. |
| `slider_float(label: &str, v: &mut f32, min: f32, max: f32) -> bool` | True when changed. |
| `input_double(label: &str, v: &mut f64) -> bool` | A number field; true when changed. |
| `input_text(label: &str, text: &mut TextBuffer, flags: i32) -> bool` | A one-line text field. |
| `input_text_multiline(label: &str, text: &mut TextBuffer, width: f32, height: f32, flags: i32) -> bool` | A text box; 0 is the default size, negative fills the window except that many pixels. |
| `same_line()`, `separator()` | Layout. |
| `collapsing_header(label: &str) -> bool` | A collapsible section; true while open. |
| `begin_child(id: &str, height: f32) -> bool`, `end_child()` | A scrolling region with a border; 0 height fills the window. Call `end_child` regardless. |
| `stick_to_bottom()` | Keeps the current region scrolled to the bottom unless the player scrolled up. |
| `begin_tab_bar(id: &str) -> bool`, `end_tab_bar()` | Tabs; call `end_tab_bar` only when `begin_tab_bar` returned true. |
| `begin_tab_item(label: &str, open: Option<&mut bool>, flags: i32) -> bool`, `end_tab_item()` | A tab; true while selected (then call `end_tab_item`). With `open`, it has a close button. |
| `begin_menu(label: &str) -> bool`, `end_menu()` | A submenu; `end_menu` only when it returned true. |
| `menu_item(label: &str, selected: bool) -> bool` | A menu entry; true when clicked. |
| `ctrl_shortcut(key: char) -> bool` | Ctrl and a letter pressed this frame, also while typing in a field. |
| `set_keyboard_focus_here(offset: i32)` | Focus the next widget (`0`) or the one just drawn (`-1`). |

| Item | |
|---|---|
| `struct TextBuffer` | Text edited by the text fields: `TextBuffer::new(s)`, `default()`, `.set(s)`, `.as_str()`. It keeps 64 KiB of spare room, the most one paste can add. |
| `text_flags::ALLOW_TAB_INPUT`, `ENTER_RETURNS_TRUE`, `READ_ONLY` | Flags for the text fields, combined with `\|`. |
| `tab_flags::UNSAVED_DOCUMENT`, `SET_SELECTED` | Flags for tabs. |

## `bind`

Used by the code that [`oxidizer --bindgen`](../rust/bindings.md) generates; you rarely use it directly.

| Item | |
|---|---|
| `struct Bindings` | `Bindings::new(version)` checks that the running game is the build the bindings were made for: `.ok()`, `.invoke(this, token, args)`, `.field_get(this, token)`, `.field_set(this, token, value)`. |
| `to_f64(Obj) -> f64`, `to_i64(Obj) -> i64`, `to_bool(Obj) -> bool` | Conversions for generated code. |

## Low level

| Item | |
|---|---|
| `type Handle = usize` | A .NET GC handle; 0 is null. |
| `struct Api` | The function table Oxidizer passes to mods (`#[repr(C)]`), fields in ABI order; the functions above wrap it. |
| `api() -> &'static Api` | The table, after `init`. |
| `read_string(f) -> String` | Reads a string from an `Api` function that fills a buffer and returns its full length. |
