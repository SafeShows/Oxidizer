# How mods are found

Every mod is a folder in the game's mods folder:

```text
Documents/My Games/Kitten Space Agency/mods/
└── my_mod/              <- the folder name is the mod's id
    ├── mod.toml
    └── main.rhai        <- a Rhai mod's script
```

When the game starts, Oxidizer looks at each folder's `mod.toml`. A mod is an Oxidizer mod when its `mod.toml` has an `[oxidizer]` section:

```toml
name = "my_mod"

[oxidizer]
script = "main.rhai"     # a Rhai mod: the script to run
# dll = "bin/my_mod"     # a Rust mod: the library, see below
```

- `script` is a Rhai file, relative to the folder.
- `dll` is a Rust mod's library, without the platform's prefix and extension: `dll = "bin/my_mod"` finds `bin/my_mod.dll` on Windows, `bin/libmy_mod.so` on Linux and `bin/libmy_mod.dylib` on a Mac, so one `mod.toml` works everywhere. Keep the library in a subfolder such as `bin/`: StarMap tries to load `mods/<id>/<id>.dll` as a C# mod, and a native library there stops the game.

A mod that is disabled in the game's mod list (`enabled = false` in `manifest.toml`) is skipped. The game asks whether to enable a new mod when it first sees it.

## Dependencies and load order

Mods start in an order: each after the mods it depends on. A mod can say what it needs, and which version, in its `[oxidizer]` section (Oxidizer 0.8 and later):

```toml
[oxidizer]
script = "main.rhai"
version = "1.2.0"                                       # this mod's version, for mods that depend on it
dependencies = [
    { id = "weather_core", min = "1.0", max = "1.9" },  # needed; versions 1.0 to 1.9 (both ends included)
    { id = "fancy_ui", optional = true },               # used if it's installed; starts first then
    "simple_dep",                                       # just the id: needed, any version
]
load_after = ["some_mod"]                               # start after some_mod if it's installed; nothing required
```

- **Order:** a mod starts after its dependencies and `load_after` mods. Rust and Rhai mods share one order, so a Rust mod can depend on a Rhai mod and the other way around. Mods that don't depend on each other start in folder-name order, the same on every machine.
- **What a dependency is:** any mod in the mods folder, by its folder name. For an Oxidizer mod it must also have started; for another mod (a C# or content mod) being installed and enabled is enough.
- **Versions:** `min` and `max` are optional and inclusive; versions compare by their numbers (`1.2` is `1.2.0`; `-beta` or `+build` suffixes are ignored). A dependency with bounds must say its own `version` in its `[oxidizer]` section.
- **When a dependency isn't met** (missing, disabled, didn't start, or the wrong version), the mod doesn't start, and **Mods > Oxidizer** says why: `needs weather_core 1.0 to 1.9, but 2.1.0 is installed`. An optional dependency that's missing is fine; one that's installed must fit its bounds.
- **Rhai mods keep checking:** a Rhai mod waiting for a dependency starts within a second of it being installed or starting. Rust mods are checked when the game starts.
- **Cycles** (a needs b, b needs a) are logged, and those mods start in folder order.

Mod managers install dependencies from the [content index](publishing/content-index.md) listing; `dependencies` in `mod.toml` is what Oxidizer itself checks when the game starts.

## While the game runs

- A Rhai mod's script is checked once a second. Save it, and the new code takes over: see [Callbacks and state](rhai/callbacks.md).
- A new Rhai mod that appears in the mods folder starts within a second, without a restart (Oxidizer 0.7.0 and later).
- Rust mods load when the game starts. To try a new build, quit the game, install it and start again.

## Settings

A mod's settings don't live in its folder, so updating the mod doesn't lose them. They go to `<game documents>/oxidizer/<mod id>.toml`: see [Rhai settings](rhai/keys-settings.md) and [Rust settings](rust/more.md).
