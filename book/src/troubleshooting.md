# Troubleshooting

Start with **Mods > Oxidizer** in the game's menu bar: it lists every Oxidizer mod with its state and shows the log.

**My mod isn't in the list.**
Check that the folder is directly in the mods folder (`mods/my_mod/mod.toml`, not `mods/my_mod/my_mod/mod.toml`), that `mod.toml` has an `[oxidizer]` section with `script` or `dll`, and that the mod isn't disabled in the game's mod list.

**A Rhai mod shows an error.**
The message has the line and position. After fixing the file, save it: the mod starts again. An error while running (in `frame`, `gui`, ...) stops the mod until the next save.

**"Function not found".**
The function or its arguments don't exist as written. Rhai is strict about types: `1` is an `int` and `1.0` a `float`, and a function that takes floats won't take ints. Look the name up in the [definitions](rhai/editor.md).

**`obj.call(...)` fails with "Data type incorrect: string (expecting Fn)".**
In Rhai, call a game method by name with `obj.invoke("Method")`; `call` belongs to Rhai's function pointers.

**A Rust mod: "built for API vN, this Oxidizer supports up to vM".**
The mod needs a newer Oxidizer: update Oxidizer.

**A Rust mod doesn't load after a rebuild.**
Rust mods load when the game starts; on Windows the library is locked while the game runs. Quit, install the new build, start again.

**The game stops when StarMap starts it, with a BadImageFormatException.**
A Rust mod's library is next to `mod.toml` with the mod's name (`mods/my_mod/my_mod.dll`), where StarMap expects a C# mod. Move it to `bin/` and set `dll = "bin/my_mod"`.

**A hotkey doesn't fire.**
Keys typed into a text field or the console don't count, and the game may use the key itself. Try another key and check its [name](rhai/keys-settings.md).

**My window is hidden behind the game's panels.**
The game draws some of its HUD over mod windows. Click your window to bring it to the front, or move it.

**Still stuck?**
Ask in the [Oxidizer forum thread](https://forums.ahwoo.com/forums/kitten-space-agency/tools/oxidizer-write-ksa-mods-in-rust-or-rhai-scripts-mod-loader.1559/) or [open an issue](https://github.com/SafeShows/Oxidizer/issues), with the log from **Mods > Oxidizer**.
