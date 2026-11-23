# Windows and menus

[`oxidizer_api::imgui`](https://docs.rs/oxidizer_api/latest/oxidizer_api/imgui/index.html) calls the game's own [Dear ImGui](https://github.com/ocornut/imgui), so mod windows share the game's look and input. Draw windows in `Mod::gui` and menu entries in `Mod::menu`.

## Windows

```rust
use oxidizer_api::imgui;

fn gui(&mut self, _dt: f64) {
    if !self.visible {
        return;
    }
    imgui::set_next_window_size(320.0, 240.0);       // the size the first time it opens
    if imgui::begin_closable("My mod", &mut self.visible) {   // a window with a close button
        imgui::text("Hello");
        imgui::text_colored([0.5, 0.9, 0.5, 1.0], "in green");
        if imgui::button("Click me") { /* ... */ }
        imgui::same_line();
        imgui::checkbox("Enabled", &mut self.enabled);
        imgui::slider_float("Warp", &mut self.warp, 1.0, 100.0);
        imgui::input_double("Altitude", &mut self.altitude);
        imgui::separator();
        if imgui::collapsing_header("More") {
            imgui::text("only while open");
        }
    }
    imgui::end();                                     // always, even when begin returned false
}
```

| Function | Does |
|---|---|
| `begin(title)`, `begin_closable(title, &mut open)`, `end()` | A window; call `end` regardless. |
| `set_next_window_size(w, h)` | The next window's size the first time it is shown. |
| `text`, `text_colored(rgba, s)` | Text. |
| `button`, `checkbox`, `slider_float`, `input_double` | Widgets; buttons return true when clicked, the others edit the value you pass. |
| `same_line`, `separator`, `collapsing_header` | Layout. |
| `begin_child(id, height)`, `end_child`, `stick_to_bottom` | A scrolling region (0 height fills the window), e.g. for a log. |

## Text fields and tabs

Text fields edit a [`TextBuffer`](https://docs.rs/oxidizer_api/latest/oxidizer_api/imgui/struct.TextBuffer.html) in place (`oxidizer_api` 0.4):

```rust
use oxidizer_api::imgui::{self, TextBuffer, text_flags};

// in your struct: name: TextBuffer, notes: TextBuffer  (TextBuffer::new("") or ::default())
if imgui::input_text("Name", &mut self.name, text_flags::ENTER_RETURNS_TRUE) {
    log(&format!("entered {}", self.name.as_str()));
}
imgui::input_text_multiline("##notes", &mut self.notes, -f32::MIN_POSITIVE, 200.0, text_flags::ALLOW_TAB_INPUT);

if imgui::begin_tab_bar("tabs") {
    if imgui::begin_tab_item("First", None, 0) {
        imgui::text("first tab");
        imgui::end_tab_item();
    }
    imgui::end_tab_bar();
}

if imgui::ctrl_shortcut('s') { /* Ctrl+S, also while typing in a field */ }
```

## The Mods menu

```rust
fn menu(&mut self) {
    if imgui::menu_item("My mod", self.visible) {   // a check mark while visible
        self.visible = !self.visible;
    }
}
```

`begin_menu(label)` / `end_menu()` make a submenu (call `end_menu` only when `begin_menu` returned true).

Widgets in one window need different labels; add `##` and something unique to show the same label twice (`"Go##1"`, `"Go##2"`).

## KSA's own UI (0.15+)

[`oxidizer_api::ksa_ui`](https://docs.rs/oxidizer_api/latest/oxidizer_api/ksa_ui/) draws with the game's own kit (API version 8, `oxidizer_api` 0.9): windows with KSA's title bar, signature tag, body and footer, labelled rows and styled controls.

```rust
use oxidizer_api::ksa_ui;

if ksa_ui::begin_window("my_mod", "My mod", "MOD", Some(&mut self.visible), 420.0, 300.0, 0) {
    ksa_ui::begin_body();
    ksa_ui::push_widget_style();
    ksa_ui::begin_row("Enabled");
    ksa_ui::checkbox("enabled", &mut self.enabled);
    ksa_ui::end_row();
    if ksa_ui::positive_button("Start") { /* ... */ }
    ksa_ui::pop_widget_style();
    ksa_ui::end_body();
    ksa_ui::begin_footer();
    ksa_ui::footer_status("Ready");
    ksa_ui::end_footer();
    ksa_ui::end_window(); // only when begin_window returned true: otherwise it is already ended
}
```

`imgui` also gained tree nodes, tables, popups, tooltips, plots, radio buttons, selectables, list boxes, int sliders, drags, `text_wrapped`, `spacing`, `begin_ex` (window flags) and `begin_child_ex` in 0.9. [ImGui Editor](https://safeshows.github.io/imgui-editor/) designs windows in the browser and writes this code.

## More than the wrappers

The wrappers cover common widgets. For anything else ImGui can do, call the game's ImGui library yourself: it exports Dear ImGui's functions by name (`Begin`, `InputTextMultiline`, `GetWindowDrawList`, ...). [Rhai Studio](https://github.com/SafeShows/oxidizer-rhai-studio)'s `gfx.rs` loads extra functions this way, for its colored editor, and its `theme.rs` styles a window like the game's own dialogs.
