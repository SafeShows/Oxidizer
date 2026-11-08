# Editor setup

## In the game: Rhai Studio

[Rhai Studio](https://github.com/SafeShows/oxidizer-rhai-studio) is an editor inside the game: syntax colors, Ctrl+S saves and reloads, errors above the editor, a new-mod wizard, and a console that runs Rhai against the live game. Install it like any mod and press F9. It also has autocomplete, an API explorer, and breakpoints (below).

### Breakpoints

Click left of a line number in Rhai Studio to set a breakpoint (Oxidizer 0.11 and Rhai Studio 1.2 or later). When the mod reaches that line, Oxidizer takes a snapshot: the function's variables, `this`, and the call stack. The **DEBUG** panel shows them, and the line is highlighted.

The game doesn't stop, only the mod: the call that hit the breakpoint finishes, and then Oxidizer stops calling the mod (its `frame`, windows, menu entries, events and hooks) until you press **CONTINUE**. A breakpoint in `frame` stops again on the next frame, so CONTINUE steps one frame at a time. Click the dot again to remove the breakpoint; a mod paused there runs on.

Other tools can do the same through the console: `debug_breakpoints("my_mod/main.rhai", [23])`, `debug_state()` (JSON) and `debug_continue("my_mod")`.

## On your desktop: definitions for autocomplete

Oxidizer describes everything scripts can use in Rhai definition files, written to:

```text
<game documents>/oxidizer/definitions/
├── ksa.d.rhai          the game's modules: ksa::Program::controlled_vehicle(), ...
├── __static__.d.rhai   game object members (v.id, v.get_surface_speed()) and Oxidizer's own functions
└── ...                 Rhai's built-in functions
```

They are rewritten whenever the game or Oxidizer updates. Each entry has its types, as scripts see them (`int`, `float`, `bool`, `String`, or a game type), and which game types have each member:

```rust
/// `KSA.Universe.SetSimulationSpeed`
fn set_simulation_speed(simulationSpeed: float, alert: bool) -> ();

/// the game method `GetBarometricAltitude` on Vehicle
fn get_barometric_altitude(obj: Obj) -> float;
```

To use them, open your mod's folder in an editor with Rhai support and add the definitions folder to the workspace. In Visual Studio Code, install a Rhai extension (search the marketplace for "Rhai") and use **File > Add Folder to Workspace...** with the definitions folder.

Even without autocomplete, the files are a searchable list of everything in the game: search them for a word like `throttle` to find what you can call.
