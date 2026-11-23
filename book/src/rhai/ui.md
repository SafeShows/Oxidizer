# Windows and menus

Mods draw with the game's own [Dear ImGui](https://github.com/ocornut/imgui), so windows look and behave like the game's.

## Windows

Draw windows in `gui(dt)`. A window is everything between `begin` and `end`; call `end()` even when `begin` returns false (the window is collapsed):

```rust
fn init() {
    this.on = false;
    this.warp = 1.0;
}

fn gui(dt) {
    if begin("My mod") {
        text("Hello");
        if button("Click me") {
            print("clicked");
        }
        same_line();
        text("next to the button");
        separator();
        this.on = checkbox("Enabled", this.on);
        this.warp = slider("Warp", this.warp, 1.0, 100.0);
        if header("More") {
            text("only while the section is open");
        }
    }
    end();
}
```

| Function | Does |
|---|---|
| `begin(title)` / `end()` | A window. The title is also its identity: two windows with one title are one window. |
| `text(s)` | A line of text. |
| `button(label)` | A button; true on the frame it is clicked. |
| `checkbox(label, value)` | A checkbox; returns the new value. |
| `slider(label, value, min, max)` | A slider for a number; returns the new value. |
| `same_line()` | Puts the next widget on the same line. |
| `separator()` | A horizontal line. |
| `header(label)` | A collapsible section; true while open. |

## More widgets

Oxidizer 0.8 and later:

```rust
fn init() {
    this.name = "Jeb";
    this.notes = "";
    this.mass = 1.5;
}

fn gui(dt) {
    set_next_window_size(420.0, 320.0);              // the size the first time the window opens
    if begin("My mod") {
        text_colored([1.0, 0.6, 0.2], "orange");     // [r, g, b] or [r, g, b, a], 0 to 1
        this.name = input_text("Name", this.name);   // returns the text after this frame's typing
        this.mass = input_float("Mass", this.mass);
        if begin_tab_bar("tabs") {
            if begin_tab("Notes") {
                this.notes = input_multiline("##notes", this.notes, 300.0, 120.0);
                end_tab();
            }
            if begin_tab("About") {
                text("second tab");
                end_tab();
            }
            end_tab_bar();
        }
        begin_child("log", 80.0);                    // a scrolling box; 0 height fills the window
        text("...");
        end_child();
        if ctrl_shortcut("s") {                      // Ctrl+S, also while typing in a field
            print("saved");
        }
    }
    end();
}
```

| Function | Does |
|---|---|
| `text_colored(color, s)` | Text in a color: `[r, g, b]` or `[r, g, b, a]`, each 0 to 1. |
| `input_text(label, text)` | A one-line text field; returns the text after this frame's edits. |
| `input_multiline(label, text, width, height)` | A multi-line text box (Tab types a tab); returns the text. A negative width or height fills the window except that many pixels. |
| `input_float(label, value)` | A number field; returns the new value. |
| `begin_tab_bar(id)` / `end_tab_bar()` | Tabs; call `end_tab_bar()` only when `begin_tab_bar` returned true. |
| `begin_tab(label)` / `end_tab()` | A tab; true while it is selected, then call `end_tab()`. |
| `begin_child(id, height)` / `end_child()` | A scrolling region with a border; call `end_child()` always. |
| `set_next_window_size(width, height)` | The next window's size the first time it is shown. |
| `ctrl_shortcut(key)` | True on the frame Ctrl and the letter were pressed, also while typing in a text field. |
| `input_int(label, value)` | A whole-number field with - and + buttons; returns the new value. 0.14+. |
| `combo(label, index, items)` | A drop-down list of strings; returns the chosen index. 0.14+. |
| `color_edit(label, color)` | A color picker for `[r, g, b, a]`; returns the new color. 0.14+. |
| `progress_bar(fraction, text)` | A bar filled to fraction (0 to 1) with text over it; `""` shows the percentage. 0.14+. |
| `set_cursor_pos(x, y)` | Puts the next widget at x, y pixels from the window's top-left corner. 0.14+. |
| `set_next_item_width(width)` | The next widget's width in pixels. 0.14+. |
| `set_next_window_pos(x, y)` | The next window's position on the screen the first time it is shown. 0.14+. |
| `tree_node(label)` / `tree_pop()` | A tree node; true while open. Call `tree_pop()` only then. 0.15+. |
| `begin_table(id, columns, flags)` | A table (flags added up: 1920 borders, 64 row stripes, 1 resizable). Then `table_setup_column(label)` per column and `table_headers_row()`, `table_next_column()` before each cell, `end_table()` only when it returned true. 0.15+. |
| `open_popup(id)` / `begin_popup(id)` / `end_popup()` | A popup: open it (e.g. when a button is clicked); `begin_popup` is true while it is open. Also `begin_popup_modal(title)`, `close_current_popup()`. 0.15+. |
| `item_tooltip(text)` | A tooltip for the widget just drawn. `is_item_hovered()` tells whether the mouse is on it. 0.15+. |
| `plot_lines(label, values, height)` / `plot_histogram(...)` | A line or bar graph of an array of numbers. 0.15+. |
| `radio_button(label, active)` | True when clicked: `if radio_button("A", this.mode == 0) { this.mode = 0; }`. 0.15+. |
| `selectable(label, selected)`, `list_box(label, index, items, rows)` | A clickable row; a list of strings (returns the chosen index). 0.15+. |
| `slider_int`, `drag_float`, `drag_int` | Whole-number slider, numbers dragged with the mouse (min == max: no bounds); return the new value. 0.15+. |
| `text_wrapped(text)`, `spacing()` | Text wrapped at the window's edge; a little vertical space. 0.15+. |
| `begin_ex(title, flags)`, `begin_child_ex(id, w, h, border)` | `begin` with window flags (added up: 1 no title bar, 2 no resize, 4 no move, 32 no collapse, 64 auto-resize); a sized child region. 0.15+. |

Store what the text and number functions return, as with `checkbox`: a field shows what you pass in, so `input_text("Name", this.name)` without storing the result would undo the player's typing every frame. Sizes are floats: write `300.0`, not `300`.

Widgets in one window need different labels. To show the same label twice, add `##` and something unique: `button("Go##1")`, `button("Go##2")`. Only the part before `##` is shown.

## KSA's own UI (0.15+)

The game draws its windows with its own kit: a title bar with a short signature tag, a body, a footer, and styled controls. The `ksa_*` functions draw with it, so a mod window looks like the rest of the game:

```rust
fn gui(dt) {
    if !this.visible { return; }
    if ksa_begin_window("my_mod", "My mod", "MOD", 420.0, 300.0, 0) {
        ksa_begin_body();
        ksa_push_widget_style();
        ksa_region_header("Orbit");
        ksa_readout("Apoapsis", "182.4 km");
        ksa_begin_row("Enabled");
        this.on = ksa_checkbox("on", this.on);
        ksa_end_row();
        ksa_begin_row("Throttle");
        this.throttle = ksa_slider_float("throttle", this.throttle, 0.0, 1.0, `${this.throttle}`);
        ksa_end_row();
        if ksa_positive_button("Start") { /* ... */ }
        ksa_pop_widget_style();
        ksa_end_body();
        ksa_begin_footer();
        ksa_footer_status("Ready");
        ksa_end_footer();
        ksa_end_window();   // only when ksa_begin_window returned true: otherwise it is already ended
    }
}
```

Controls: `ksa_button`, `ksa_primary_button`, `ksa_positive_button`, `ksa_danger_button`, `ksa_checkbox(id, v)`, `ksa_slider_float` / `ksa_slider_int` / `ksa_drag_float` / `ksa_drag_int(id, v, min, max, text)` (text is what the control shows), `ksa_combo(id, index, items)`, `ksa_text_line(id, s)`, `ksa_color_edit(id, [r, g, b])`, `ksa_segmented(id, items, index)`, `ksa_readout(key, value)`, `ksa_value_text(text)`, `ksa_region_header(label)`, `ksa_rule()`, `ksa_item_tooltip(text)`. Like the plain widgets, the value functions return the new value: store it. Put a control in `ksa_begin_row(label)` / `ksa_end_row()` to give it a label. Plain widgets work in a KSA window too: `ksa_push_widget_style()` gives them KSA's look.

[ImGui Editor](https://safeshows.github.io/imgui-editor/) designs these windows in the browser and writes the code (tick *KSA look*).

## The Mods menu

`menu()` runs while the game's **Mods** menu is open. Add entries with `menu_item`:

```rust
fn menu() {
    if menu_item("My mod", this.visible) {   // shown with a check mark while this.visible is true
        this.visible = !this.visible;
    }
}
```

`menu_item(label)` is an entry without a check mark. For a submenu, `if begin_menu("My mod") { ...entries...; end_menu(); }` (call `end_menu` only when `begin_menu` returned true).

The common pattern is a window that the menu entry shows and hides, as above, plus `if !this.visible { return; }` at the start of `gui`.
