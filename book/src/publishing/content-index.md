# The content index

The [KSA content index](https://github.com/KSAModding/content-index) is the list that mod managers such as Borea install from, shown on [ksamods.gg](https://ksamods.gg). Listing your mod there means players can find it and install it, with Oxidizer, in a few clicks.

## Steps

1. **Publish a release first.** The index checks your newest release when you open the pull request, and a listing that points at a release that doesn't exist yet fails.
2. **Post a forum thread** for the mod on the [KSA forums](https://forums.ahwoo.com) (Mod Releases or Tools). Every listing links to one. The board's rules ask for a license, the source code and attribution in the post.
3. **Write the listing:** copy your template's `index-listing.example.toml` to `listings/<id>.toml` in a fork of the content index, fill it in, and open a pull request. Automated checks run on it, and when they pass it is merged on its own.

New releases after that are picked up automatically; edit the listing only when its facts change.

## The loader

A listing names one loader, the thing that must be running for the mod to work. An Oxidizer mod can be listed either way:

```toml
# For players who start the game with StarMap: the manager installs StarMap and Oxidizer.StarMap.
[loader]
id = "StarMap"
min = "0.4.6"

[[dependencies]]
id = "Oxidizer.StarMap"
kind = "required"
min = "0.7.0"
```

```toml
# For players who use standalone Oxidizer.
[loader]
id = "Oxidizer"
min = "0.7.0"
```

The mod itself runs on both; the choice only decides which players a mod manager offers it to. [RFC 0127 (loader bridges)](https://github.com/KSAModding/content-manager-design/pull/127) is the proposal for one listing that works with both: once it is accepted and mod managers support it, Oxidizer mods name `[loader] id = "Oxidizer"`, and a manager uses `Oxidizer.StarMap` for players on StarMap.

## A finished listing

[Flight Panel](https://github.com/SafeShows/oxidizer-flight-panel/blob/main/index-listing.example.toml) and [Rhai Studio](https://github.com/SafeShows/oxidizer-rhai-studio/blob/main/index-listing.example.toml) keep their merged listings in their repositories, with a description, a screenshot (`[[images.description]]`, with the image's size and hash), and the forum thread.
