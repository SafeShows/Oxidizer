# Hooks

A hook runs your function when the game calls one of its methods: before it (a prefix) or after it (a postfix).

```rust
use oxidizer_api::{Obj, hook, log};

fn init() -> Self {
    // (type, method, postfix?, function)
    hook("KSA.Program", "OnFocus", false, on_focus);
    MyMod {}
}

/// The object the method runs on (null for a static method) and an array of its arguments.
fn on_focus(_program: &Obj, args: &Obj) -> bool {
    log(&format!("focus: {}", args.index(1)));
    true // run the game's method too; ignored for postfixes
}
```

`hook` returns true when the patch is in place. Set up hooks once, in `init`. The function is a plain `fn` (or a closure that captures nothing).

## Skipping a method and changing results

A prefix that returns `false` skips the game's method. Inside a hook, [`hook_result()`](https://docs.rs/oxidizer_api/latest/oxidizer_api/fn.hook_result.html) is the method's return value and [`set_hook_result(&value)`](https://docs.rs/oxidizer_api/latest/oxidizer_api/fn.set_hook_result.html) replaces it:

```rust
use oxidizer_api::{Obj, hook, set_hook_result};

hook("KSA.Universe, KSA", "CanAutoWarp", true, |_, _| {
    set_hook_result(&Obj::from(false)); // the game now thinks it can't auto-warp
    true
});
```

From a prefix, `set_hook_result` plus returning `false` skips the method and returns your value.

## Hooks and your mod's state

Hooks are called from inside the game's calls, so they don't get `&mut self`. Keep what they read in statics and update it from `frame`:

```rust
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

static LIMIT_ON: AtomicBool = AtomicBool::new(true);

fn before_increase(_: &Obj, _: &Obj) -> bool {
    !LIMIT_ON.load(Relaxed) // false (skip) while the limit is on
}
```

[Flight Panel](https://github.com/SafeShows/oxidizer-flight-panel)'s `warp_guard.rs` uses this pattern to stop time warp at low altitude.

## Overloads and the raw form

- When the method has overloads, name the one you mean: `"SetSimulationSpeed(double, bool)"`.
- [`patch(type, method, postfix, callback, user)`](https://docs.rs/oxidizer_api/latest/oxidizer_api/fn.patch.html) is the raw form with an `extern "C"` callback and a user value, for when a plain `fn` isn't enough.
- A panic in a hook is caught and the game's method runs as usual.
