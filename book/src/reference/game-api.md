---
hide:
  - navigation
  - toc
---

# Game API

Every public type and member of the game (`KSA.dll`) that mods can use, as [`oxidizer --bindgen`](../rust/bindings.md) generates them: the C# member, the typed Rust binding, and how to reach it from Rhai.

<div id="game-api" markdown="0">
  <input id="game-api-search" type="search" placeholder="Search types and members, e.g. Vehicle, throttle, SetSimulationSpeed" autocomplete="off">
  <p id="game-api-status">Loading the game's API…</p>
  <div id="game-api-results"></div>
</div>

## Reading it

- **Rust** is the function in the bindings file that `oxidizer --bindgen` writes. Paths start at the generated module (`ksa::Vehicle`); objects of other types are passed and returned as their wrapper types, or `Obj` when the type has no wrapper.
- **Rhai** is the [typed access](../rhai/game.md) Rhai mods get: `ksa::Type::member()` for static members, `obj.member` and `obj.method()` on objects. Static overloads with the same number of arguments get `_2`, `_3`, ... in Rhai in their own order; the [definition files](../rhai/editor.md) have the exact names.
- Members that can't cross to mods (pointers, `ref` parameters, open generics) aren't listed. The by-name API (`static_call`, `obj.invoke` in Rhai, `Obj::call` in Rust) reaches everything else, including private members.

This page shows the game version named above the results. After a game update, it is regenerated with `oxidizer --bindgen book/src/assets/game-api.json`.
