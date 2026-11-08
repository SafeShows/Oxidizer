# Releases

Players and mod managers install mods from releases: a zip on your GitHub repository's **Releases** page, made from a git tag.

## The zip

A mod's zip holds one folder named after the mod's id, laid out as it goes into the mods folder:

```text
my_mod-1.0.0.zip
└── my_mod/
    ├── mod.toml
    ├── main.rhai                 (a Rhai mod)
    ├── bin/my_mod.dll            (a Rust mod: one library per platform)
    ├── bin/libmy_mod.so
    ├── bin/libmy_mod.dylib
    ├── LICENSE
    └── THIRD-PARTY-NOTICES.txt   (a Rust mod: the licenses of the crates compiled in)
```

Name it `<id>-<version>.zip`, version without a `v`.

## With the templates

Both templates have a release workflow. Push a tag and it publishes the release:

```sh
git tag v1.0.0
git push origin v1.0.0
```

- **Rhai** ([template](https://github.com/SafeShows/oxidizer-rhai-mod-template)): zips the mod folder.
- **Rust** ([template](https://github.com/SafeShows/oxidizer-rust-mod-template)): builds the library on Windows, Linux and macOS, writes `THIRD-PARTY-NOTICES.txt` from the crates the library contains (forum rules ask for that attribution), and zips everything.

A manual run of the workflow (**Actions > release > Run workflow**) builds the zip as a downloadable artifact without publishing anything, to check it first.

## Before you publish

- Pick a license and put it in `LICENSE`. The templates use MIT No Attribution, which lets you relicense freely.
- Try the zip: unzip it into a clean mods folder and start the game.
- Note which Oxidizer version you need: a Rust mod built with `oxidizer_api` 0.4 needs Oxidizer 0.7.0 or later; a Rhai mod needs the version that added the functions it uses.
