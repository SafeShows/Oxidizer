# Changelog

All notable changes to Oxidizer and `oxidizer_api` are listed here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Events and services between mods, Rust and Rhai alike. Events: `publish`, `publish_retained` (kept as the topic's current value for later subscribers) and `subscribe`, delivered at the start of the next frame. Services: `provide` and `call_service` / `call`, which return a value. Names start with the owning mod's id, and only that mod publishes on or provides them. Values are JSON: maps in Rhai, serde types in Rust (`Event` and `Service` traits). **Mods > Oxidizer** has an **Event bus** section. Examples: `bus_rust` and `bus_rhai`.
- `oxidizer_api` 0.6.0 (API version 5): the bus and services. Each Rust mod gets its own API table, which carries its id. Mods built with older versions keep working.

## [0.8.0] - 2026-11-08

### Added

- `loaded()`: a callback once the game has loaded its content and mods (after `KSA.ModLibrary.LoadAll`, the moment of StarMap's `AllModsLoaded`), for Rhai (`fn loaded()`) and Rust (`Mod::loaded`, `oxidizer_api` 0.5, API version 4). `init` stays where it was, before the game's `Main`. A Rhai mod that starts later gets `loaded()` right after `init()`.
- More ImGui for Rhai: `text_colored`, `input_text`, `input_multiline`, `input_float`, tabs (`begin_tab_bar`, `begin_tab`), `begin_child`, `set_next_window_size` and `ctrl_shortcut`.
- Dependencies and load order between mods, in `mod.toml`'s `[oxidizer]` section: `dependencies` (each optional or required, with optional `min` / `max` versions), `load_after`, and the mod's own `version`. Mods start in dependency order, Rust and Rhai mods in one order; a mod whose dependencies aren't met doesn't start and **Mods > Oxidizer** says why. Rhai mods waiting for a dependency start as soon as it's there.
- `oxidizer --bindgen <file>.json` writes the game's API as data: every type and member with its C# signature, its Rust binding and its Rhai form. The [modding guide](https://safeshows.github.io/Oxidizer/)'s Game API page is built from it.

## [0.7.0] - 2026-11-08

### Added

- Script mods that appear in the mods folder while the game runs start within a second, without a restart (for example one made by an in-game editor).
- `oxidizer_api` 0.4.0 (API version 3): read Oxidizer's log (`log_lines`, `log_next`) and mod states (`mod_statuses`), run Rhai code against the live game (`rhai_eval`, a console whose variables persist), `mods_dir`; ImGui text fields (`input_text`, `input_text_multiline` with `TextBuffer`), tabs and `ctrl_shortcut`. Mods built with it need this Oxidizer or later; older mods keep working.

### Changed

- The Rhai engine is built when it is first needed, so a game without Rhai mods doesn't spend the startup time on it.

## [0.6.3] - 2026-11-08

### Changed

- Rhai editor definitions now carry types for instance members too (`fn get_surface_speed(obj: Obj) -> float`), with the game types that have each member in its description. A member that game types define differently keeps `?`. Types are named as scripts see them: `int`, `float`, `bool`, `String`, or the game type.
- The definitions are rewritten when Oxidizer updates too, not only when the game does.

## [0.6.2] - 2026-11-08

### Fixed

- Rhai: calling a game method by name is now `obj.invoke(method[, args])`. The documented `obj.call(...)` never worked, because Rhai keeps `x.call(...)` for function pointers ("Data type incorrect: string (expecting Fn)").
- Rhai: a method call on a game property (`vehicle.parent.get("MeanRadius")`) no longer logs "Property set method not found". Rhai writes such properties back afterwards; an unchanged value is no longer written.

## [0.6.1] - 2026-11-07

### Added

- Standalone Oxidizer on macOS: `oxidizer-macos` finds the [macOS port](https://github.com/renancamm/ksa-macos-port)'s app in `~/Applications` (or `/Applications`) and restarts itself with the environment the port's launcher sets up (its .NET runtime, GLFW/KTX shims and Metal hooks), so it runs with no setup. Tested on an M3 Pro.

## [0.6.0] - 2026-11-07

### Added

- Typed game access for Rhai, built from the installed game at startup: `ksa::Program::controlled_vehicle()`, `vehicle.id`, `vehicle.throttle = 0.5`, `ksa::Universe::set_simulation_speed(2.0, false)`, enum values like `ksa::VehicleEngine::MainIgnite`. Numbers, bools and strings come back as Rhai values. Takes about 0.4 s at startup for the whole game (about 32,000 functions).
- Rhai definition files for editors, written to `<documents>/oxidizer/definitions/` on each new game version, so editors with Rhai support can autocomplete the game and Oxidizer's functions.
- Overloads can be named with their parameter types in `static_call`, `call` and `patch`: `"SetSimulationSpeed(double, bool)"`. Without them, the overload whose parameter types match the arguments is picked, and errors list the overloads.

### Changed

- Rhai scripts no longer block each other: a script calling a method that another script hooks now runs that hook (before, it was skipped). Only a script hooking a method it calls itself is still skipped while it runs.
- Patching finds private methods of base classes too.
- The bridge is built against `StarMap.API` 0.5.2 (compile-only, as before). Tested with StarMap 0.5.2: Oxidizer's **Mods** menu merges with StarMap 0.5's built-in one, so players see a single menu with both C# and Oxidizer entries.

## [0.5.1] - 2026-11-07

### Added

- macOS (Apple Silicon), with the [macOS port](https://github.com/renancamm/ksa-macos-port) of the game: the StarMap mod has `liboxidizer_core.dylib`, and works with the port's StarMap (tested on an M3 Pro with macOS 27: Rhai and Rust mods, hooks and hot reload). The standalone zip has `oxidizer-macos`, which needs the port's launcher to start it and isn't tested yet.
- Standalone Oxidizer finds .NET through `DOTNET_ROOT`, or in the app bundle's `Contents/MacOS`, when the game folder has no `hostfxr` of its own (the macOS port uses Microsoft's shared runtime).

## [0.5.0] - 2026-11-07

### Added

- Oxidizer runs as a StarMap mod: `Oxidizer.StarMap-<version>.zip` holds an `Oxidizer.StarMap` folder for `mods/`. StarMap starts the game and loads it like any C# mod, and Oxidizer then runs the Rhai and Rust mods, side by side with StarMap's C# mods. Standalone Oxidizer stays as it was.
- Oxidizer's core is also a library (`oxidizer_core.dll` / `liboxidizer_core.so`), which the StarMap mod loads.

### Changed

- Rust mods keep their library in a subfolder, `dll = "bin/my_mod"`: StarMap tries to load `mods/<id>/<id>.dll` as a C# mod, and a native library there stops the game. `dll` paths may now include folders.
- Release zips are named `Oxidizer-<version>.zip` and `Oxidizer.StarMap-<version>.zip` (no `v`), which the content index needs to tell them apart.

### Removed

- The `[loader]` table in the example `mod.toml` files: nothing reads it there. Loaders are declared in the content index listing.

## [0.4.0] - 2026-11-07

Rust mods: the new API is in `oxidizer_api` 0.3.0.

### Added

- Typed Rust bindings: `oxidizer --bindgen ksa.rs` writes a Rust module with a type for every public `KSA.dll` type and methods for its public methods, properties, fields and constructors, plus constants for enums. Calls go by exact metadata token, so overloads are no longer guessed, and the file refuses to run against a different game build.
- Hooks can read and replace the hooked method's return value: `result()` / `set_result(v)` in Rhai, `hook_result()` / `set_hook_result(&v)` in Rust.
- Hotkeys: `key_pressed(name)` and `key_down(name)`, using the game's key names. Keys typed into text fields, the console or a dialog don't count.
- Settings: `config_get(key, default)` / `config_set(key, value)` in Rhai, saved to `<documents>/oxidizer/<mod>.toml`; `config_path(mod_id)` for Rust mods.
- `Obj::i64()`, and `Obj::from` for `i64` and `bool`.

### Changed

- API version 2. Mods built for API version 1 keep working; mods built for version 2 need this Oxidizer or newer.
- Releases have one zip, `Oxidizer-<version>.zip`, with the Windows and the Linux build. (0.4.0 was first published with separate Windows and Linux zips too; those were removed because the content index can't use them.)

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

[Unreleased]: https://github.com/SafeShows/Oxidizer/compare/v0.8.0...HEAD
[0.8.0]: https://github.com/SafeShows/Oxidizer/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/SafeShows/Oxidizer/compare/v0.6.3...v0.7.0
[0.6.3]: https://github.com/SafeShows/Oxidizer/compare/v0.6.2...v0.6.3
[0.6.2]: https://github.com/SafeShows/Oxidizer/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/SafeShows/Oxidizer/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/SafeShows/Oxidizer/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/SafeShows/Oxidizer/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/SafeShows/Oxidizer/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/SafeShows/Oxidizer/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/SafeShows/Oxidizer/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/SafeShows/Oxidizer/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/SafeShows/Oxidizer/releases/tag/v0.1.0
