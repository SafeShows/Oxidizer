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

Store what the text and number functions return, as with `checkbox`: a field shows what you pass in, so `input_text("Name", this.name)` without storing the result would undo the player's typing every frame. Sizes are floats: write `300.0`, not `300`.

Widgets in one window need different labels. To show the same label twice, add `##` and something unique: `button("Go##1")`, `button("Go##2")`. Only the part before `##` is shown.

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
