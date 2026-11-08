# Typed bindings

Instead of member names in strings, a Rust mod can use generated Rust types for the whole game, with autocomplete and compile-time checks. Standalone Oxidizer writes them from your installed game:

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

- Each wrapper derefs to its base class and finally to `Obj`, so inherited members and the [by-name API](game.md) work on it too.
- Members are called by their exact metadata token, so there's no overload guessing and no lookup by name at runtime.
- Only public members are generated; use the by-name API for the rest.

## The catch: one game build

The file only works with the game build it was generated from. Against another build, every call does nothing after one log message telling you to regenerate it. So:

- regenerate it after each game update and release a new build of your mod, or
- use the by-name API for mods you publish, and bindings for exploring and for personal mods.

The Rust mod template keeps `src/ksa.rs` out of git, because it belongs to one game version and CI has no game to generate it; a release build needs it committed. [Flight Panel](https://github.com/SafeShows/oxidizer-flight-panel) uses the by-name API for this reason.
