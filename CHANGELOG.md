# Changelog

All notable changes to Oxidizer and `oxidizer_api` are listed here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.0] - 2026-11-07

### Added

- Linux support. Releases include `Oxidizer-<version>-linux.zip` next to the Windows zip.
- `dll` in `mod.toml` can be written without an extension (`dll = "my_mod"`), and loads `my_mod.dll` on Windows or `libmy_mod.so` on Linux. A name with `.dll` still works, and falls back to the Linux library on Linux.

### Changed

- ImGui calls go through the plain C functions the game itself uses, instead of C++ names that only exist on Windows. This is in `oxidizer_api` 0.2.1: Rust mods that should run on Linux need to be built with it. Mods built with 0.2.0 keep working on Windows.
- The example mods declare `[loader] id = "Oxidizer"` in their `mod.toml`, which mod managers such as Borea read.

## [0.2.0] - 2026-11-07

### Added

- A **Mods** menu in the game's menu bar. **Mods > Oxidizer** opens a window that lists the loaded mods with their state and errors, and shows the log.
- Mods can add their own entries to the Mods menu: `menu()` in Rhai, `Mod::menu` in Rust.
- Rhai scripts reload while the game runs when their file is saved. State in `this` is kept, `init()` doesn't run again, and an optional `reload()` runs instead. If the new version doesn't compile, the old one keeps running and the error shows in the Oxidizer window.
- A script that failed to compile at startup, or stopped after an error, starts again once its file is saved.
- Instances: `-InstancePath <folder>` or the `OXIDIZER_INSTANCE_PATH` environment variable makes the game use that folder for mods, saves and settings instead of `Documents/My Games/Kitten Space Agency`. Mod managers such as Borea use this for separate game profiles.
- `oxidizer_api`: ImGui menus (`begin_menu`, `end_menu`, `menu_item`), closable windows, `set_next_window_size`, scrolling child regions and colored text.
- The release zip includes `THIRD-PARTY-NOTICES.txt` with the licenses of Harmony, the libraries bundled in it, and every Rust crate compiled into `oxidizer.exe`.

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

[Unreleased]: https://github.com/SafeShows/Oxidizer/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/SafeShows/Oxidizer/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/SafeShows/Oxidizer/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/SafeShows/Oxidizer/releases/tag/v0.1.0
