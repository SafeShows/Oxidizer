# Changelog

All notable changes to Oxidizer and `oxidizer_api` are listed here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-11-07

First release.

### Added

- `oxidizer.exe`, a launcher that starts Kitten Space Agency through the game's own .NET runtime and loads mods before the game starts. The game folder comes from `--game <folder>` or from `game_path` in `oxidizer.toml`, which is created next to the exe on first run.
- Mods are loaded from `Documents/My Games/Kitten Space Agency/mods/<id>/` when their `mod.toml` has an `[oxidizer]` section naming a `dll` or a `script`. Mods disabled in the game's `manifest.toml` are skipped.
- Rust mods: a `cdylib` crate that depends on [`oxidizer_api`](https://crates.io/crates/oxidizer_api), implements the `Mod` trait (`init`, `frame`, `gui`, `unload`) and registers with `oxidizer_mod!`.
- Rhai script mods: a `.rhai` file with optional `init()`, `frame(t, dt)`, `gui(dt)` and `unload()` functions, and per-mod state in `this`.
- Access to game objects by name: read and write static and instance fields and properties, call methods, and index collections. Arguments are converted to the parameter types.
- Patching game methods before (prefix) or after (postfix) they run, from Rust and from Rhai. A prefix can skip the original method.
- ImGui windows drawn with the game's own ImGui: windows, text, buttons, checkboxes, sliders, number input, separators and collapsing headers.
- API version check: a Rust mod built for a different API version is skipped with a message to rebuild it, instead of crashing the game.
- Example mods `hello_rust` and `hello_rhai`, included in the release zip.

### Known limitations

- Windows only.
- Oxidizer launches the game itself, so it can't run together with StarMap.
- When a method has several overloads, `call` and patching use the first one with a matching number of arguments.
- Hooks can't read or change a patched method's return value.

[Unreleased]: https://github.com/SafeShows/Oxidizer/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/SafeShows/Oxidizer/releases/tag/v0.1.0
