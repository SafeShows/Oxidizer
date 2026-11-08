# Hooks

A hook runs your function when the game calls one of its methods: before it (a prefix) or after it (a postfix). Hooks are how a mod changes what the game does.

```rust
fn init() {
    patch("KSA.Program", "OnFocus", "prefix", "on_focus");
}

// Called with the object the method runs on (a null Obj for a static method) and an array of its arguments.
fn on_focus(program, args) {
    print(`focus: ${args[1]}`);
    true   // run the game's method too
}
```

`patch(type, method, "prefix" | "postfix", "function name")` returns true when the patch is in place. Set up patches once, in `init()`. The function is called by name, so after a hot reload the new version of it runs.

## Skipping a method

A prefix that returns `false` skips the game's method:

```rust
fn init() {
    patch("KSA.Universe", "IncreaseSimulationSpeed", "prefix", "limit_warp");
}

fn limit_warp(universe, args) {
    // Don't warp past 4x.
    ksa::Universe::get_simulation_speed() < 4.0
}
```

## Return values

Inside a hook, `result()` is the method's return value (in a postfix, what the game's method returned) and `set_result(value)` replaces it:

```rust
fn init() {
    patch("KSA.Universe", "CanAutoWarp", "postfix", "no_auto_warp");
}

fn no_auto_warp(universe, args) {
    set_result(false);   // the game now thinks it can't auto-warp
}
```

In a prefix, combine `set_result(value)` with returning `false` to skip the method and return your value instead.

## Overloads and limits

- When the method has overloads, name the one you mean: `patch("KSA.Universe", "SetSimulationSpeed(double, bool)", "prefix", "f")`. If it is ambiguous, the error lists the overloads.
- A script's hook doesn't run while that same script is running further up, that is, when a script hooks a method it calls itself: the call goes through unhooked. Hooks of other scripts, and calls from the game and from Rust mods, always run.
- Patches stay until the game closes. To change what a hook does, change its function and save.
