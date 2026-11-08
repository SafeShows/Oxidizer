# Files and command line

## `mod.toml`

In each mod's folder (`mods/<id>/mod.toml`). The game reads `name`; Oxidizer reads `[oxidizer]`.

```toml
name = "my_mod"

[oxidizer]
script = "main.rhai"   # a Rhai mod: the script, relative to the folder
dll = "bin/my_mod"     # a Rust mod: the library; the platform's prefix and extension are added if this file doesn't exist
```

Optional keys of `[oxidizer]` (Oxidizer 0.8 and later; see [Dependencies and load order](../mod-folders.md#dependencies-and-load-order)):

| Key | |
|---|---|
| `version = "1.2.0"` | This mod's version, for mods that depend on it with `min` / `max`. |
| `dependencies = [...]` | Mods to start after. Each is `"id"` or `{ id = "id", optional = true, min = "1.0", max = "2.0" }`; `optional` defaults to false, the bounds are inclusive. |
| `load_after = ["id", ...]` | Mods to start after when they're installed, without needing them. |

A mod may have both `dll` and `script`. A folder without `[oxidizer]` isn't an Oxidizer mod (it may be a C# or content mod). The folder name is the mod's id.

## `oxidizer.toml` (standalone)

Next to the `oxidizer` program; written on the first run.

```toml
game_path = 'C:\Program Files\Ahwoo\Games\Kitten Space Agency'   # the folder with KSA.dll
```

## Command line (standalone)

```text
oxidizer [--game <folder>] [-InstancePath <folder>] [game arguments...]
oxidizer [--game <folder>] --bindgen <file.rs>
```

| Option | |
|---|---|
| `--game <folder>` | The game folder, instead of `game_path` in `oxidizer.toml`. |
| `-InstancePath <folder>` | Use this folder instead of the game's documents folder (mods, saves, settings), as mod managers do for instances. Also `OXIDIZER_INSTANCE_PATH`. |
| `--bindgen <file.rs>` | Write [typed Rust bindings](../rust/bindings.md) for the installed game to the file, without starting the game. |

Other arguments go to the game.

| Environment variable | |
|---|---|
| `OXIDIZER_INSTANCE_PATH` | The instance folder, like `-InstancePath`. |

## Where things are

| Path | |
|---|---|
| `<documents>/mods/<id>/` | A mod. `<documents>` is `Documents/My Games/Kitten Space Agency`, or the instance folder. |
| `<documents>/manifest.toml` | The game's mod list; `enabled = false` disables a mod. |
| `<documents>/oxidizer/<id>.toml` | A mod's settings (Rhai `config_set`, Rust `config_path`). |
| `<documents>/oxidizer/definitions/` | [Rhai definition files](../rhai/editor.md) for editors. |
| `<documents>/logs/` | The game's own logs. On the macOS port, game and Oxidizer output goes to `logs/console.log`. |
