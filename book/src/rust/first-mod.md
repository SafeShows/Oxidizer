# Your first Rust mod

A Rust mod is a library (`cdylib`) that Oxidizer loads, built with the [`oxidizer_api`](https://docs.rs/oxidizer_api) crate. You need [Rust](https://rustup.rs).

The quickest start is the [Rust mod template](https://github.com/SafeShows/oxidizer-rust-mod-template): click **Use this template** on GitHub, clone your new repository, and you have a working mod, install scripts, and a release workflow that builds for Windows, Linux and macOS. The rest of this page shows what is in it.

## The pieces

`Cargo.toml`:

```toml
[package]
name = "my_mod"          # also the mod's id: its folder name in mods/
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
oxidizer_api = "0.4"
```

`src/lib.rs`:

```rust
use oxidizer_api::{Mod, Obj, imgui, log};

struct MyMod {
    clicks: u32,
}

impl Mod for MyMod {
    fn init() -> Self {
        log("my_mod: hello!");
        MyMod { clicks: 0 }
    }

    fn gui(&mut self, _dt: f64) {
        if imgui::begin("My mod") {
            let v = Obj::static_get("KSA.Program, KSA", "ControlledVehicle");
            imgui::text(&format!("vehicle: {}", if v.is_null() { "none".into() } else { v.get("Id").to_string() }));
            if imgui::button("Click me") {
                self.clicks += 1;
            }
            imgui::same_line();
            imgui::text(&format!("clicks: {}", self.clicks));
        }
        imgui::end();
    }
}

oxidizer_api::oxidizer_mod!(MyMod);
```

and `mod/mod.toml`, which goes into the mod's folder next to the library:

```toml
name = "my_mod"

[oxidizer]
dll = "bin/my_mod"       # bin/my_mod.dll, bin/libmy_mod.so or bin/libmy_mod.dylib
```

## Build and install

```sh
cargo build --release
```

Then copy `mod/mod.toml` to `mods/my_mod/mod.toml`, and the library from `target/release/` (`my_mod.dll`, `libmy_mod.so` or `libmy_mod.dylib`) to `mods/my_mod/bin/`. The template's `install.ps1` (Windows) and `install.sh` (Linux, macOS) do both.

You don't need to quit the game to install a new build (Oxidizer 0.10 and later): within about a second of the library changing, Oxidizer reloads the mod. See [hot reload](mod-trait.md#hot-reload). **Mods > Oxidizer** shows whether the mod loaded and its log lines.

## Versions

`oxidizer_api` 0.4 needs Oxidizer 0.7.0 or later; a mod built for a newer API than the installed Oxidizer is refused with a message to update Oxidizer. Mods built with an older `oxidizer_api` keep working on newer Oxidizer versions.
