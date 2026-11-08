# Using the game

The whole game is available to scripts as typed modules, built from your installed game when Oxidizer starts, so they always match your game version.

## Static members: `ksa::...`

Each namespace and type of the game is a module, with its static members as functions, named in snake_case:

```rust
let v = ksa::Program::controlled_vehicle();       // KSA.Program.ControlledVehicle
let t = ksa::Universe::get_elapsed_seconds();     // KSA.Universe.GetElapsedSeconds()
ksa::Universe::set_simulation_speed(2.0, false);  // an overload with two arguments
```

- A static property or field `Foo` is `foo()`, and `set_foo(value)` sets it when it can be set.
- Constructors are `ksa::SomeType::new(...)`.
- Overloads with different numbers of arguments share a name. Overloads with the same number get `_2`, `_3`, ... (see the [definitions](editor.md) for which is which).
- Enum values are integer constants: `ksa::VehicleEngine::MainIgnite`.
- Nested namespaces nest modules: `KSA.Rendering.Foo` is `ksa::rendering::Foo`.

## Instance members

Game objects have their properties, fields and methods, also in snake_case:

```rust
let v = ksa::Program::controlled_vehicle();
if v != () {
    print(v.id);                         // a property
    print(v.get_surface_speed());        // a method
    v.show_ground_track = true;          // set a property that can be set
    print(v.parent.id);                  // chain through objects
}
```

## What comes back

- Numbers, bools and strings come back as Rhai values (`int`, `float`, `bool`, `String`); enums as `int`.
- `null` comes back as `()`: check with `x == ()`.
- Everything else is an `Obj`, a game object you can use further.

## By name

The typed modules cover the game's public members. Anything else, including private members and other libraries' static members, is reachable by name:

```rust
let v = static_get("KSA.Program, KSA", "ControlledVehicle");       // static property or field
static_set("KSA.SimSpeed, KSA", "MaxSpeed", 100.0);
let t = static_call("KSA.Universe, KSA", "GetElapsedSeconds");     // static method
static_call("KSA.Universe, KSA", "SetSimulationSpeed", [5.0, true]);

let id = v.get("Id");                // member of an object
v.set("ShowGroundTrack", true);
let s = v.invoke("GetSurfaceSpeed"); // method; v.invoke("Name", [args]) with arguments
```

By-name calls return an `Obj` even for numbers: use `.float()` for its number, `.to_string()` for text, `.is_null()`, `.len()` and `obj[i]` for collections.

When a method has overloads, name the one you mean with its parameter types: `static_call("KSA.Universe, KSA", "SetSimulationSpeed(double, bool)", [5.0, true])`. C# keywords (`double`, `bool`, `int`, `string`), short type names and full names all work. Without types, Oxidizer picks the overload that fits the arguments, and if it can't tell, the error lists them.

## Finding names

- The [editor definitions](editor.md) list every function and member, with types, for autocomplete.
- A .NET decompiler such as [ILSpy](https://github.com/icsharpcode/ILSpy) on the game folder's `KSA.dll` shows what members do.
- [Rhai Studio](https://github.com/SafeShows/oxidizer-rhai-studio)'s console runs a line against the live game, so you can try things: `ksa::Program::controlled_vehicle().id`.
