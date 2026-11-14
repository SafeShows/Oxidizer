# Rhai API reference

Everything Oxidizer adds to Rhai, as of Oxidizer 0.9. The language itself (variables, strings, arrays, maps, loops, Rhai's built-in functions) is in the [Rhai book](https://rhai.rs/book/).

Types below are written as Rhai sees them: `int`, `float`, `bool`, `String`, `Array`, `()` (nothing), `Obj` (a game object), and `?` for any value.

## Callbacks

Functions your script defines; Oxidizer calls them. All are optional. See [Callbacks and state](../rhai/callbacks.md).

| Callback | Called |
|---|---|
| `init()` | Once, when the mod starts, before the game's `Main` (not again on hot reload). |
| `loaded()` | Once the game has loaded its content and mods (0.8+). Mods that start later get it right after `init()`. |
| `frame(t: float, dt: float)` | Every frame. `t`: the game's player time; `dt`: seconds since the last frame. |
| `gui(dt: float)` | Every frame, inside the ImGui frame. |
| `menu()` | While the game's **Mods** menu is open. |
| `reload()` | After the file was saved and reloaded. |
| `unload()` | When the game closes. |
| `saving()` | The game is about to write a save: set what it should remember with `save_set` (0.12+). |
| `save_loaded()` | The game loaded a save: read it with `save_get` (0.12+). |

Inside callbacks, `this` is the mod's object map; it keeps its values across calls and hot reloads.

## The game, typed (`ksa::`)

Built at startup from the installed game; the exact list is in the [definition files](../rhai/editor.md). The rules:

| Game member | In Rhai |
|---|---|
| Namespace `KSA`, `KSA.Rendering` | module `ksa`, `ksa::rendering` |
| Static property or field `Foo` of type `T` | `ksa::T::foo()`, and `ksa::T::set_foo(value)` when settable |
| Static method `GetFoo(a, b)` | `ksa::T::get_foo(a, b)`; overloads with the same number of arguments get `_2`, `_3`, ... |
| Constructor | `ksa::T::new(...)` |
| Enum value `E.Value` | `ksa::E::Value`, an `int` constant |
| Instance property or field `Foo` | `obj.foo`, and `obj.foo = value` when settable |
| Instance method `GetFoo(a)` | `obj.get_foo(a)` (the overload is chosen by the arguments) |

Names are the game's, in snake_case (`GetHTTPName` → `get_http_name`); names that Rhai or Oxidizer already use get a `_` (a member `Print` is `print_`).

Values coming back from the game: numbers, bools and strings become `int`, `float`, `bool`, `String`; enums `int`; `null` `()`; anything else an `Obj`.

## Game objects by name

| Function | Returns | Does |
|---|---|---|
| `static_get(type: String, member: String)` | `Obj` | Static property or field. `type` is a full name, e.g. `"KSA.Program, KSA"` (namespace.type, assembly). |
| `static_set(type: String, member: String, value: ?)` | `bool` | Sets a static property or field; true on success. |
| `static_call(type: String, method: String)` | `Obj` | Static method without arguments. |
| `static_call(type: String, method: String, args: Array)` | `Obj` | Static method with arguments. |
| `obj.get(member: String)` | `Obj` | Property or field of an object. |
| `obj.set(member: String, value: ?)` | `bool` | Sets a property or field. |
| `obj.invoke(method: String)` | `Obj` | Method without arguments. (Not `call`: Rhai keeps `call` for function pointers.) |
| `obj.invoke(method: String, args: Array)` | `Obj` | Method with arguments. |
| `obj.float()` | `float` or `()` | The number (also bools and enums), or `()` when it isn't one. |
| `obj.is_null()` | `bool` | Whether the object is null. |
| `obj.len()` | `int` | Length of an array or collection; `-1` otherwise. |
| `obj[i]` | `Obj` | Element `i` of an array or list. |
| `obj.to_string()` | `String` | The object's `ToString()`; also used by `print` and string interpolation. |

A method name may name an overload by its parameter types: `"SetSimulationSpeed(double, bool)"`. Arguments are converted to the parameter types.

## Hooks

| Function | Returns | Does |
|---|---|---|
| `patch(type: String, method: String, kind: String, function: String)` | `bool` | Runs your function `function(instance: Obj, args: Obj)` before (`kind = "prefix"`) or after (`"postfix"`) the game's method. A prefix returning `false` skips the method. True when the patch is in place. |
| `result()` | `Obj` | Inside a hook: the method's return value. |
| `set_result(value: ?)` | `bool` | Inside a hook: replaces the method's return value. |

See [Hooks](../rhai/hooks.md).

## Keys and settings

| Function | Returns | Does |
|---|---|---|
| `key_pressed(key: String)` | `bool` | True on the frame the key went down. Key names: see [Keys and settings](../rhai/keys-settings.md). |
| `key_down(key: String)` | `bool` | True while the key is held. |
| `config_get(key: String, default: ?)` | `?` | The saved value, or `default`. |
| `config_set(key: String, value: bool \| int \| float \| String)` | `()` | Saves a value; an error for other types. |
| `save_get(key: String, default: ?)` | `?` | This mod's value in the current save game, or `default`. 0.12+. |
| `save_set(key: String, value: ?)` | `()` | Sets this mod's value in the current save game, written with the next save; `()` removes it. Numbers, strings, bools, arrays and maps. 0.12+. |

## Windows (inside `gui`)

| Function | Returns | Does |
|---|---|---|
| `begin(title: String)` | `bool` | Starts a window; false while collapsed. Always call `end()`. |
| `end()` | `()` | Ends the window. |
| `text(s: String)` | `()` | A line of text. |
| `button(label: String)` | `bool` | A button; true when clicked. |
| `checkbox(label: String, value: bool)` | `bool` | A checkbox; returns the new value. |
| `slider(label: String, value: float, min: float, max: float)` | `float` | A slider; returns the new value. |
| `same_line()` | `()` | The next widget goes on the same line. |
| `separator()` | `()` | A horizontal line. |
| `header(label: String)` | `bool` | A collapsible section; true while open. |
| `text_colored(color: Array, s: String)` | `()` | Text in a color, `[r, g, b]` or `[r, g, b, a]` (0..1). 0.8+. |
| `input_text(label: String, text: String)` | `String` | A one-line text field; returns the edited text. 0.8+. |
| `input_multiline(label: String, text: String, width: float, height: float)` | `String` | A multi-line text box; returns the edited text. 0.8+. |
| `input_float(label: String, value: float)` | `float` | A number field; returns the new value. 0.8+. |
| `begin_tab_bar(id: String)` / `end_tab_bar()` | `bool` / `()` | Tabs; `end_tab_bar()` only when it returned true. 0.8+. |
| `begin_tab(label: String)` / `end_tab()` | `bool` / `()` | A tab; true while selected, then `end_tab()`. 0.8+. |
| `begin_child(id: String, height: float)` / `end_child()` | `bool` / `()` | A scrolling region; always call `end_child()`. 0.8+. |
| `set_next_window_size(width: float, height: float)` | `()` | The next window's first size. 0.8+. |
| `ctrl_shortcut(key: String)` | `bool` | Ctrl plus the letter this frame, also while typing. 0.8+. |

## The Mods menu (inside `menu`)

| Function | Returns | Does |
|---|---|---|
| `menu_item(label: String)` | `bool` | An entry; true when clicked. |
| `menu_item(label: String, selected: bool)` | `bool` | An entry with a check mark while `selected`. |
| `begin_menu(label: String)` | `bool` | A submenu; call `end_menu()` only when it returned true. |
| `end_menu()` | `()` | Ends the submenu. |

## Events and services

Names start with the owning mod's id; only that mod publishes on or provides them. See [Events and services](../events.md). 0.9+.

| Function | Returns | Does |
|---|---|---|
| `publish(topic: String, value: ?)` | `()` | Sends `value` to the topic's subscribers, at the start of the next frame. |
| `publish_retained(topic: String, value: ?)` | `()` | The same, and keeps it as the topic's current value for later subscribers. |
| `subscribe(topic: String, function: String)` | `()` | Calls `function(value)` in this script for each event on the topic (and the retained value, if any). Once per topic and function. |
| `unsubscribe(topic: String, function: String)` | `()` | Stops that. |
| `provide(name: String, function: String)` | `()` | Lets other mods call `function(args)` in this script as the service `name`; its return value is the result. |
| `call_service(name: String, args: ?)` | `?` | Calls a service and returns its value; throws its error. |
| `call_service(name: String)` | `?` | The same, with `()` as the arguments. |

Values are numbers, strings, bools, arrays and object maps (they cross as JSON). `subscribe` and `provide` work in a script, not in the console.

## Inspecting the game

| Function | Returns | Does |
|---|---|---|
| `inspect(value: ?)` | `String` | What a game object holds right now, as JSON: its type, its public properties and fields (type, current value, static, whether it can be set) and its first 100 items if it is a collection. Rhai Studio's INSPECT panel shows it. 0.12+. |

## Breakpoints

What [Rhai Studio](../rhai/editor.md#breakpoints) uses; a source is `"mod/file.rhai"`. 0.11+.

| Function | Returns | Does |
|---|---|---|
| `debug_breakpoints(source: String, lines: Array)` | `()` | Sets a script's breakpoint lines (empty clears them). A mod that reaches one is paused until `debug_continue`. |
| `debug_state()` | `String` | The breakpoints, and what each saw when last hit (variables, `this`, call stack, whether the mod is paused), as JSON. |
| `debug_continue(mod_id: String)` | `()` | Lets a paused mod run again. |

## Output

| Function | Does |
|---|---|
| `print(x)` | Writes to Oxidizer's log (**Mods > Oxidizer**). |
| `debug(x)` | The same, with Rhai's debug formatting. |
