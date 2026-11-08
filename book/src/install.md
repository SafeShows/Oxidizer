# Installing Oxidizer

Oxidizer comes in two forms. Both run the same mods, from the same folder.

| | As a StarMap mod | Standalone |
|---|---|---|
| Listing | `Oxidizer.StarMap` | `Oxidizer` |
| Starts the game | [StarMap](https://github.com/StarMapLoader/StarMap) | `oxidizer.exe` (`oxidizer` on Linux, `oxidizer-macos` on a Mac) |
| C# (StarMap) mods too | yes | no |

**With a mod manager** such as Borea, install the Oxidizer listing you want, or just a mod that needs it: the manager brings Oxidizer along.

**By hand**, download the zips from the [releases](https://github.com/SafeShows/Oxidizer/releases):

- **StarMap mod:** unzip `Oxidizer.StarMap-X.Y.Z.zip` into `Documents/My Games/Kitten Space Agency/mods/`, so there is a `mods/Oxidizer.StarMap/` folder, and start the game through StarMap.
- **Standalone:** unzip `Oxidizer-X.Y.Z.zip` anywhere and run `oxidizer.exe`. If the game isn't in the usual place, set `game_path` in the `oxidizer.toml` it creates next to the program. (Linux has no usual place; on a Mac the default is the [macOS port](https://github.com/renancamm/ksa-macos-port)'s app in `~/Applications`.)

Then start the game. In the menu bar, **Mods > Oxidizer** opens Oxidizer's window: every Oxidizer mod with its state (running, or why not), and the log. You'll use it a lot while writing mods.

> **Example mods:** the standalone zip has `example-mods/hello_rhai` and `example-mods/hello_rust`. Copy them into the mods folder to see a Rhai and a Rust mod running.
