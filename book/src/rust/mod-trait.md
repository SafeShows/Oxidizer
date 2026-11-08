# The Mod trait

A Rust mod is a type that implements [`oxidizer_api::Mod`](https://docs.rs/oxidizer_api/latest/oxidizer_api/trait.Mod.html), registered with `oxidizer_mod!`:

```rust
use oxidizer_api::Mod;

struct MyMod { /* your state */ }

impl Mod for MyMod {
    fn init() -> Self { MyMod {} }                        // once, when the game starts
    fn frame(&mut self, player_time: f64, dt: f64) {}     // every frame
    fn gui(&mut self, dt: f64) {}                         // every frame, inside the ImGui frame: draw windows
    fn menu(&mut self) {}                                 // while the game's Mods menu is open
    fn unload(&mut self) {}                               // when the game closes
}

oxidizer_api::oxidizer_mod!(MyMod);
```

Only `init` is required. Your state lives in the struct; every callback gets `&mut self`.

`oxidizer_mod!` exports the functions Oxidizer looks for in the library. It also catches panics: a panic in a callback is logged ("mod panicked in gui") instead of taking the game down, and the next callback runs as usual.

## State that hooks need

[Hooks](hooks.md) run inside the game's own calls, not in your callbacks, so they don't get `&mut self`. Keep what they need in statics, such as atomics or a `Mutex`, and update it from `frame`. [Flight Panel](https://github.com/SafeShows/oxidizer-flight-panel)'s `warp_guard.rs` is a complete example.
