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
