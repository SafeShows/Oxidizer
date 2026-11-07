# Oxidizer

A mod loader for Kitten Space Agency, written in Rust. Mods can be written in Rust (cdylib) or as Rhai scripts. Runs on Windows, Linux and macOS (Apple Silicon, with the [macOS port](https://github.com/renancamm/ksa-macos-port)), on its own or as a [StarMap](https://github.com/StarMapLoader/StarMap) mod next to C# mods.

![The two example mods, one in Rhai and one in Rust, drawing ImGui windows in game](docs/screenshot.png)

KSA is a .NET 10 game, so Oxidizer has a small C# bridge (with Harmony) that hooks `Program.OnFrame`, `Program.OnDrawUiViewports` and the game's menu bar and gives mods access to game objects, and a native core that runs the Rhai and Rust mods. It starts in one of two ways:
- **As a StarMap mod** (`Oxidizer.StarMap`): StarMap starts the game and loads the bridge like any C# mod, and the bridge loads the core. Use this if you use StarMap or other C# mods.
- **Standalone** (`oxidizer`): Oxidizer starts the game's .NET runtime itself (`hostfxr.dll` / `libhostfxr.so` from the game folder), loads the bridge, then runs the game's `Main`. No StarMap needed.

In game, **Mods > Oxidizer** in the menu bar opens a window that lists the loaded mods, their errors and the log.

## Install

The [releases](https://github.com/SafeShows/Oxidizer/releases) have two zips, each with the Windows, Linux and macOS builds.

**As a StarMap mod:** unzip `Oxidizer.StarMap-X.Y.Z.zip` into `Documents/My Games/Kitten Space Agency/mods/` (so there is a `mods/Oxidizer.StarMap/` folder) and start the game through StarMap as usual.

**On a Mac:** set up the [macOS port](https://github.com/renancamm/ksa-macos-port) with its StarMap (`./starmap.sh`), then install the StarMap mod as above. If you downloaded the zip in a browser, clear the quarantine flag or macOS won't load the libraries: `xattr -dr com.apple.quarantine ~/Documents/My\ Games/Kitten\ Space\ Agency/mods`.

**Standalone:**
1. Unzip `Oxidizer-X.Y.Z.zip` anywhere.
2. Run `oxidizer.exe` (Linux: `./oxidizer`). If KSA isn't installed at the default path, set `game_path` in the `oxidizer.toml` it creates next to the exe. On Linux there is no default, so set it on the first run.

To try the example mods, copy the folders from `example-mods/` in the standalone zip into `Documents/My Games/Kitten Space Agency/mods/`. Mod managers such as Borea install either one for you.

## Build

```sh
dotnet build bridge -c Release -o target/bridge
cargo build --release --workspace
```

## Run

```sh
target/release/oxidizer.exe [--game "<KSA folder>"] [-InstancePath "<folder>"] [game args...]
```

On first run without `--game`, the loader creates `oxidizer.toml` next to the exe (`game_path = '...'`). On Windows it defaults to `C:\Program Files\Ahwoo\Games\Kitten Space Agency`; on Linux, set it yourself.

`oxidizer --bindgen ksa.rs` writes typed Rust bindings for the installed game instead of starting it (see [Typed bindings](#typed-bindings)).

`-InstancePath`, or the `OXIDIZER_INSTANCE_PATH` environment variable, makes the game use that folder instead of `Documents/My Games/Kitten Space Agency` for mods, saves and settings. Mod managers such as Borea use it for separate game profiles.

## Mods

**Starting a mod:** use a template, with a working example, a release workflow and the content-index listing:
[Rhai](https://github.com/SafeShows/oxidizer-rhai-mod-template) or
[Rust](https://github.com/SafeShows/oxidizer-rust-mod-template) (**Use this template** on GitHub).


A mod is a folder in `Documents/My Games/Kitten Space Agency/mods/<id>/` whose `mod.toml` has a `[oxidizer]` section:

```toml
name = "my_mod"

[oxidizer]
dll = "bin/my_mod"      # Rust mod: bin/my_mod.dll on Windows, bin/libmy_mod.so on Linux
script = "main.rhai"    # and/or Rhai script
```

Mods set `enabled = false` in KSA's `manifest.toml` are skipped. See `examples/` for working mods. To install one, copy its folder into `mods/`; for a Rust mod, also copy the built library into its `bin/` folder (`.dll` on Windows, `lib*.so` on Linux). A mod for both platforms can ship both files.

Keep a Rust mod's library in a subfolder like `bin/`, not next to `mod.toml`: StarMap tries to load `mods/<id>/<id>.dll` as a C# mod, and a native library there stops the game.

**Listing a mod in the content index:** Oxidizer mods are listed as StarMap mods that need `Oxidizer.StarMap`, so mod managers install everything they need:

```toml
[loader]
id = "StarMap"

[[dependencies]]
id = "Oxidizer.StarMap"
kind = "required"
min = "0.5.0"
```

The same mod also runs with standalone Oxidizer.

### Rust

The crate is a `cdylib` that depends on `oxidizer_api`. The API crate has its own version, separate from Oxidizer's: it only changes when something changes for mod authors, so `oxidizer_api = "0.3"` works with Oxidizer 0.4 and later, and `cargo update` picks up fixes. `Mod` has `init`, `frame`, `gui`, `menu` (entries in the game's Mods menu) and `unload`; all but `init` are optional.

```rust
use oxidizer_api::{Mod, Obj, imgui};
struct M;
impl Mod for M {
    fn init() -> Self { M }
    fn gui(&mut self, _dt: f64) {
        if imgui::begin("My mod") {
            let v = Obj::static_get("KSA.Program, KSA", "ControlledVehicle");
            imgui::text(&v.get("Id").to_string());
        }
        imgui::end();
    }
}
oxidizer_api::oxidizer_mod!(M);
```

Also in `oxidizer_api`:
- `hook(type, method, postfix, fn)` patches a game method; inside the hook, `hook_result()` / `set_hook_result(&v)` read and replace its return value (from a prefix, return `false` as well to skip the original).
- `key_pressed("F5")` is true on the frame a key was pressed, `key_down("F5")` while it's held. Names are the game's key names (`"K"`, `"F5"`, `"Space"`, `"LeftShift"`, ...). Keys typed into text fields or the console, or eaten by a dialog, don't count.
- `config_path("my_mod")` is where to keep your settings: `<documents>/oxidizer/my_mod.toml`, outside the mod folder so updating the mod doesn't lose them.

#### Typed bindings

Instead of member names in strings, Rust mods can use generated types with autocomplete and compile-time checks:

```sh
oxidizer --bindgen src/ksa.rs     # once, and again after each game update
```

```rust
mod ksa; // the generated file: a module per namespace, a struct per public KSA.dll type
use ksa::ksa::{Program, Universe};

let v = Program::controlled_vehicle();          // -> ksa::ksa::Vehicle
if !v.is_null() {
    oxidizer_api::log(&format!("{} at {:.0} s", v.id(), Universe::get_elapsed_seconds()));
}
Universe::set_simulation_speed_2(5.0, true);   // overloads get _2, _3, ... suffixes
```

Each wrapper derefs to its base class and finally to `Obj`, so inherited members and the by-name API work on it too. Members are called by their exact metadata token, so there's no overload guessing. The file only works with the game build it was generated from: against another build, every call does nothing after one log message telling you to regenerate it. Only public members are generated; use the by-name API for the rest.

### Rhai

A script can define `init()`, `frame(t, dt)`, `gui(dt)`, `menu()` (entries in the game's Mods menu) and `unload()`. Keep state in `this` (an object map).

Scripts reload while the game runs: save the file and the new code takes over within a second. `this` is kept, `init()` doesn't run again, and `reload()` runs if the script defines it. If the new version doesn't compile, the old one keeps running and the error shows in the Oxidizer window.

| Area | Functions |
|---|---|
| Game objects | `static_get(type, member)`, `static_set(type, member, v)`, `static_call(type, method[, args])`, `obj.get(m)`, `obj.set(m, v)`, `obj.call(m[, args])`, `obj.float()`, `obj.is_null()`, `obj.len()`, `obj[i]`, `obj.to_string()` |
| Patching | `patch(type, method, "prefix"\|"postfix", "fn_name")`. The callback is `fn_name(instance, args)`; returning `false` from a prefix skips the original method. |
| ImGui | `begin(title)` / `end()`, `text`, `button`, `checkbox(label, v) -> v`, `slider(label, v, min, max) -> v`, `same_line`, `separator`, `header` |
| Menu (in `menu()`) | `menu_item(label[, checked]) -> clicked`, `begin_menu(label)` / `end_menu()` for a submenu |
| Return values (in a hook) | `result()`, `set_result(v)`: read or replace the hooked method's return value |
| Keys | `key_pressed(name)` (this frame), `key_down(name)` (held), e.g. `key_pressed("F5")` |
| Settings | `config_get(key, default)`, `config_set(key, value)` for bools, numbers and strings, saved to `<documents>/oxidizer/<mod>.toml` |
| Logging | `print(...)` |

## Notes and limits

- Members are looked up by name through reflection. Type names can be assembly-qualified (`"KSA.Universe, KSA"`), which works even before the game has loaded that type.
- `call` picks the first overload with a matching argument count and converts arguments to the parameter types.
- Patching uses the first overload of the method. Hooks receive the instance and the arguments, and can read and replace the return value.
- A Rhai hook doesn't run while another Rhai script is running (they share one script engine): if a script calls a method another script hooks, the call goes through unhooked. Calls from the game and from Rust mods are always hooked.
- A Rhai script that throws an error stops until its file is saved again, so the same error isn't logged every frame. Errors from reflection calls are logged once each.
- `ref struct` members (for example `ReadOnlySpan` properties such as `Program.VehiclesInFrame`) can't be read through reflection.
