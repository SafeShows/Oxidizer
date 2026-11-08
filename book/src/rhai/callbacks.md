# Callbacks and state

Oxidizer calls functions your script defines. All of them are optional.

| Function | When |
|---|---|
| `init()` | Once, when the mod starts. |
| `frame(t, dt)` | Every frame. `t` is the game's player time, `dt` the seconds since the last frame. |
| `gui(dt)` | Every frame, inside the game's ImGui frame: draw your windows here (see [Windows and menus](ui.md)). |
| `menu()` | While the game's **Mods** menu is open: add your entries here. |
| `reload()` | After you saved the file and it was reloaded. |
| `unload()` | When the game closes. |

Code at the top level of the file (outside functions) runs every time the file is loaded, before `init` or `reload`.

## Keeping state in `this`

Variables declared with `let` inside a function are gone when it returns. For state that lasts, use `this`, an object map that belongs to your mod:

```rust
fn init() {
    this.frames = 0;
    this.visible = true;
}

fn frame(t, dt) {
    this.frames += 1;
}

fn gui(dt) {
    if !this.visible { return; }
    if begin("Counter") {
        text(`frames: ${this.frames}`);
    }
    end();
}
```

`this` survives hot reloads: when you save the file, the new code runs with the same `this`, and `init()` doesn't run again (`reload()` does). So you can change how a window looks while the mod keeps counting. To start over, restart the game, or reset values in `reload()`.

## Errors

A script that doesn't compile when the game starts doesn't run; fix the file and save, and it starts. When a function stops with an error while running, the mod stops until you save the file again, so an error in `frame` or `gui` isn't repeated every frame. **Mods > Oxidizer** shows the error with its line; in [Rhai Studio](https://github.com/SafeShows/oxidizer-rhai-studio) it is above the editor.

`print(...)` writes to Oxidizer's log, which you see in **Mods > Oxidizer**.
