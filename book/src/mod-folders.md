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

## While the game runs

- A Rhai mod's script is checked once a second. Save it, and the new code takes over: see [Callbacks and state](rhai/callbacks.md).
- A new Rhai mod that appears in the mods folder starts within a second, without a restart (Oxidizer 0.7.0 and later).
- Rust mods load when the game starts. To try a new build, quit the game, install it and start again.

## Settings

A mod's settings don't live in its folder, so updating the mod doesn't lose them. They go to `<game documents>/oxidizer/<mod id>.toml`: see [Rhai settings](rhai/keys-settings.md) and [Rust settings](rust/more.md).
