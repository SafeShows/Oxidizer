# The Mod trait

A Rust mod is a type that implements [`oxidizer_api::Mod`](https://docs.rs/oxidizer_api/latest/oxidizer_api/trait.Mod.html), registered with `oxidizer_mod!`:

```rust
use oxidizer_api::Mod;

struct MyMod { /* your state */ }

impl Mod for MyMod {
    fn init() -> Self { MyMod {} }                        // once, before the game's Main: nothing loaded yet
    fn loaded(&mut self) {}                               // once the game has loaded its content and mods (0.5+)
    fn frame(&mut self, player_time: f64, dt: f64) {}     // every frame
    fn gui(&mut self, dt: f64) {}                         // every frame, inside the ImGui frame: draw windows
    fn menu(&mut self) {}                                 // while the game's Mods menu is open
    fn unload(&mut self) {}                               // when the game closes, or before a hot reload
}

oxidizer_api::oxidizer_mod!(MyMod);
```

Only `init` is required. `init` runs before the game's `Main`, so the game hasn't loaded any data yet: do setup that needs the game's parts, systems or mods in `loaded` (`oxidizer_api` 0.5, Oxidizer 0.8). No flight exists at `loaded` either; the controlled vehicle comes later, so check for it in `frame`. Your state lives in the struct; every callback gets `&mut self`.

`oxidizer_mod!` exports the functions Oxidizer looks for in the library. It also catches panics: a panic in a callback is logged ("mod panicked in gui") instead of taking the game down, and the next callback runs as usual.

## Hot reload

Rebuild the mod and copy the library into its `bin/` folder while the game runs (the template's install scripts do that), and within about a second Oxidizer reloads it (0.10 and later):

1. the running version's `unload` runs,
2. its [hooks](hooks.md), [event subscriptions and services](../events.md) are removed,
3. the new version starts: `init`, then `loaded` if the game has already loaded.

The log says `reloaded rust mod my_mod`. If the new library can't load (built for a newer API, say), the running version keeps going and the log says why.

What to know:

- **State starts over:** the new version gets a fresh `init`. To keep something across a reload, save it in `unload` (for example with `config_path`) and read it back in `init`.
- Oxidizer runs every Rust mod from a copy in `<documents>/oxidizer/loaded/`, so the file in the mod folder isn't locked. The old copy stays loaded until the game closes: a thread your mod started keeps running the old code, so stop your threads in `unload`.

## State that hooks need

[Hooks](hooks.md) run inside the game's own calls, not in your callbacks, so they don't get `&mut self`. Keep what they need in statics, such as atomics or a `Mutex`, and update it from `frame`. [Flight Panel](https://github.com/SafeShows/oxidizer-flight-panel)'s `warp_guard.rs` is a complete example.
