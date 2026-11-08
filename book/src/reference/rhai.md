# Rhai API reference

Everything Oxidizer adds to Rhai, as of Oxidizer 0.7. The language itself (variables, strings, arrays, maps, loops, Rhai's built-in functions) is in the [Rhai book](https://rhai.rs/book/).

Types below are written as Rhai sees them: `int`, `float`, `bool`, `String`, `Array`, `()` (nothing), `Obj` (a game object), and `?` for any value.

## Callbacks

Functions your script defines; Oxidizer calls them. All are optional. See [Callbacks and state](../rhai/callbacks.md).

| Callback | Called |
|---|---|
| `init()` | Once, when the mod starts (not again on hot reload). |
| `frame(t: float, dt: float)` | Every frame. `t`: the game's player time; `dt`: seconds since the last frame. |
| `gui(dt: float)` | Every frame, inside the ImGui frame. |
| `menu()` | While the game's **Mods** menu is open. |
| `reload()` | After the file was saved and reloaded. |
| `unload()` | When the game closes. |

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

## The Mods menu (inside `menu`)

| Function | Returns | Does |
|---|---|---|
| `menu_item(label: String)` | `bool` | An entry; true when clicked. |
| `menu_item(label: String, selected: bool)` | `bool` | An entry with a check mark while `selected`. |
| `begin_menu(label: String)` | `bool` | A submenu; call `end_menu()` only when it returned true. |
| `end_menu()` | `()` | Ends the submenu. |

## Output

| Function | Does |
|---|---|
| `print(x)` | Writes to Oxidizer's log (**Mods > Oxidizer**). |
| `debug(x)` | The same, with Rhai's debug formatting. |
