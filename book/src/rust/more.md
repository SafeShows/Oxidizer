# Keys, settings and more

## Keys

```rust
use oxidizer_api::{key_down, key_pressed};

fn frame(&mut self, _t: f64, _dt: f64) {
    if key_pressed("F8") {        // true only on the frame the key went down
        self.visible = !self.visible;
    }
    if key_down("LeftShift") { /* held */ }
}
```

Key names are the game's (GLFW's): `"A"` to `"Z"`, `"Number0"` to `"Number9"`, `"F1"` to `"F25"`, `"Space"`, `"Enter"`, `"Escape"`, `"LeftShift"`, `"LeftControl"`, `"LeftAlt"`, `"Up"`, `"Down"`, and so on. Keys typed into a text field or the game's console don't count. For shortcuts inside your own text fields, use [`imgui::ctrl_shortcut`](ui.md#text-fields-and-tabs).

## Settings

[`config_path("my_mod")`](https://docs.rs/oxidizer_api/latest/oxidizer_api/fn.config_path.html) is where to keep your settings: `<game documents>/oxidizer/my_mod.toml`, outside the mod's folder so updating the mod doesn't lose them. Read and write it with your own code, for example with the `toml` crate:

```rust
use oxidizer_api::config_path;

fn load() -> bool {
    std::fs::read_to_string(config_path("my_mod"))
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|t| t.get("visible")?.as_bool())
        .unwrap_or(true)
}

fn save(visible: bool) {
    let _ = std::fs::write(config_path("my_mod"), format!("visible = {visible}\n"));
}
```

## Log, mod states and Rhai

- [`log(text)`](https://docs.rs/oxidizer_api/latest/oxidizer_api/fn.log.html) writes to Oxidizer's log (**Mods > Oxidizer**).

With `oxidizer_api` 0.4 (Oxidizer 0.7.0 and later):

- [`log_lines(from)`](https://docs.rs/oxidizer_api/latest/oxidizer_api/fn.log_lines.html) and `log_next()` read the log: keep `log_next()`'s number and ask for the lines from there next time.
- [`mod_statuses()`](https://docs.rs/oxidizer_api/latest/oxidizer_api/fn.mod_statuses.html) lists every Oxidizer mod, `Rust` or `Rhai`, and why it isn't running if it isn't.
- [`rhai_eval(code)`](https://docs.rs/oxidizer_api/latest/oxidizer_api/fn.rhai_eval.html) runs Rhai against the live game, with everything Rhai mods can use, and returns its value as text or the error. Variables persist between calls, like a console.
- [`mods_dir()`](https://docs.rs/oxidizer_api/latest/oxidizer_api/fn.mods_dir.html) is the game's mods folder.

[Rhai Studio](https://github.com/SafeShows/oxidizer-rhai-studio) is built on these: its console is `rhai_eval` and `log_lines`, and its status labels come from `mod_statuses`.
