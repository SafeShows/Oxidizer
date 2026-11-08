# Typed bindings

Instead of member names in strings (`v.call("GetSurfaceSpeed", &[])`), a Rust mod can use generated Rust types for the whole game, with autocomplete and compile-time checks (`v.get_surface_speed()`). `oxidizer --bindgen` writes them from your installed game. The [Game API](../reference/game-api.md) page lists everything it generates.

## Setting it up

Bindgen is part of **standalone** Oxidizer (the `oxidizer` program from `Oxidizer-X.Y.Z.zip`); it reads the game, it doesn't start it.

1. Run it from your mod's folder, with the path to your mod's `src/`:

    ```sh
    path/to/oxidizer --bindgen src/ksa.rs
    ```

    If the game isn't in the usual place, add `--game <game folder>` or set `game_path` in `oxidizer.toml`. It prints how many types and members it wrote: `bindgen: wrote 2193 types, 26518 members to src/ksa.rs`.

2. Add the module to your crate, in `src/lib.rs`:

    ```rust
    mod ksa;
    ```

3. Use it:

    ```rust
    use ksa::ksa::{Program, Universe};

    let v = Program::controlled_vehicle();              // a ksa::ksa::Vehicle
    if !v.is_null() {
        let speed = v.get_surface_speed();              // f64
        let body = v.parent();                          // a typed wrapper too
        oxidizer_api::log(&format!("{} at {speed:.0} m/s", v.id()));
    }
    Universe::set_simulation_speed_2(5.0, true);
    ```

The first `ksa` is the module you added (the file), the second is the game's `KSA` namespace: `KSA.Rendering.Foo` is `ksa::ksa::rendering::Foo`.

## What the file contains

For each public game type, a wrapper struct around [`Obj`](game.md) with a method per member:

```rust
/// `KSA.Vehicle` (class).
#[repr(transparent)]
pub struct Vehicle(pub Obj);
impl std::ops::Deref for Vehicle {          // to the base class, and finally to Obj
    type Target = super::ksa::Astronomical;
    // ...
}
impl Vehicle {
    pub fn periapsis(&self) -> f64 { /* ... */ }                     // property Periapsis
    pub fn set_show_ground_track(&self, v: bool) { /* ... */ }       // its setter
    pub fn get_surface_speed(&self) -> f64 { /* ... */ }             // method GetSurfaceSpeed()
    pub fn parent(&self) -> super::ksa::IParentBody { /* ... */ }    // returns another wrapper
    // ...
}
```

- **Names:** snake_case; a property `Foo` is `foo()` and `set_foo(v)`; constructors are `new`.
- **Overloads:** the second overload with a name gets `_2`, the third `_3`, in the order the game declares them: `set_simulation_speed` and `set_simulation_speed_2`. Check which is which in the file or on the [Game API](../reference/game-api.md) page.
- **Types:** numbers become `f64` or `i64` (enums too), `bool` stays `bool`, strings are `&str` in and `String` out. Other game types are their wrappers, or `Obj` when they have none.
- **Enums:** a module of constants: `ksa::ksa::VehicleEngine::MainIgnite` is an `i64`.
- **Inheritance:** each wrapper derefs to its base class, so inherited members work, and finally to `Obj`, so the [by-name API](game.md) works on it too.
- **Structs** (value types) are boxed copies: a setter changes the copy, not the original.
- **Null:** a wrapper can hold null, like `Obj`; check `is_null()` before using objects that may not exist.

Members are called by their exact metadata token, so there's no overload guessing and no name lookup at runtime.

## After a game update

The file belongs to the game build it was made from. On another build, every call does nothing and logs one message asking to regenerate it, so the mod doesn't crash but doesn't work either. After each game update:

1. run `oxidizer --bindgen src/ksa.rs` again,
2. fix whatever no longer compiles (members the game renamed or removed),
3. build and release your mod again.

## Releasing a mod that uses bindings

The [Rust mod template](https://github.com/SafeShows/oxidizer-rust-mod-template) lists `src/ksa.rs` in `.gitignore`, because the release workflow runs in CI, where there's no game to generate it from. To release a mod that uses bindings, take the line out of `.gitignore` and commit the file (about 4 MB), so CI builds with it.

Because of the update step above, bindings suit mods you update with the game, and exploring. For a mod that should keep working across game updates without a new release, use the by-name API, which finds members when it runs; [Flight Panel](https://github.com/SafeShows/oxidizer-flight-panel) does that. You can mix the two: bindings while writing, by-name calls in the released code.

## As data

`oxidizer --bindgen file.json` (a `.json` file name) writes the same API as data instead of Rust: every type, with each member's C# signature, Rust function and Rhai form. The [Game API](../reference/game-api.md) page is built from it.

## Troubleshooting

- **"typed bindings were generated for another KSA build"** in the log: regenerate the file (see above).
- **A member is missing:** bindgen covers public members whose types can cross to mods. Use the by-name API for private members and members with `ref` parameters or pointers.
- **Slow first build:** the file is large; the first build of the crate takes a little longer, later builds don't.
