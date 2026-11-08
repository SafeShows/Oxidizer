# Using the game

[`Obj`](https://docs.rs/oxidizer_api/latest/oxidizer_api/struct.Obj.html) is a game object, held by Oxidizer and released when the `Obj` is dropped. You reach members by name:

```rust
use oxidizer_api::Obj;

// Static members: the type name, assembly-qualified when it isn't found otherwise ("Namespace.Type, Assembly").
let v = Obj::static_get("KSA.Program, KSA", "ControlledVehicle");
let t = Obj::static_call("KSA.Universe, KSA", "GetElapsedSeconds", &[]).f64().unwrap_or(0.0);
Obj::static_call("KSA.Universe, KSA", "SetSimulationSpeed", &[&Obj::from(5.0), &Obj::from(true)]);
Obj::static_set("KSA.SimSpeed, KSA", "MaxSpeed", &Obj::from(100.0));

// Instance members.
if !v.is_null() {
    let id = v.get("Id").to_string();
    let speed = v.call("GetSurfaceSpeed", &[]).f64();
    v.set("ShowGroundTrack", &Obj::from(true));
    let radius = v.get("Parent").get("MeanRadius").f64();
}
```

| Method | Does |
|---|---|
| `Obj::static_get(type, member)` / `Obj::static_set(type, member, &value)` | Static property or field. |
| `Obj::static_call(type, method, &[args])` | Static method. |
| `obj.get(member)` / `obj.set(member, &value)` | Property or field of an object. |
| `obj.call(method, &[args])` | Method of an object. |
| `obj.f64()`, `obj.i64()` | The number (also bools and enums), or `None`. |
| `obj.to_string()` | The object's `ToString()` (also through `{}` in `format!`). |
| `obj.is_null()` | Whether it is null. |
| `obj.len()`, `obj.index(i)` | Arrays and collections. |
| `Obj::from(x)` | A game value from `f64`, `i64`, `bool` or `&str`, for arguments and `set`. |

Arguments are converted to the parameter types (a number to `float`, `int` or an enum, and so on). When a method has overloads, name the one you mean with its parameter types: `"SetSimulationSpeed(double, bool)"`; without them, Oxidizer picks the overload that fits the arguments, and if it can't tell, the error lists them.

By-name calls find members when they run, so a mod keeps working across game updates as long as the names exist. For names with autocomplete and compile-time checks, see [typed bindings](bindings.md).

## Wrapping it up

Mods usually put the calls they need behind small typed functions, so the rest of the code reads like Rust:

```rust
pub struct Vehicle(Obj);

impl Vehicle {
    pub fn controlled() -> Option<Vehicle> {
        let v = Obj::static_get("KSA.Program, KSA", "ControlledVehicle");
        (!v.is_null()).then_some(Vehicle(v))
    }
    pub fn altitude(&self) -> f64 {
        self.0.call("GetBarometricAltitude", &[]).f64().unwrap_or(f64::NAN)
    }
}
```

[Flight Panel](https://github.com/SafeShows/oxidizer-flight-panel)'s `game.rs` does this for a whole flight data panel.

## Finding names

- A .NET decompiler such as [ILSpy](https://github.com/icsharpcode/ILSpy) on the game folder's `KSA.dll`.
- The [typed bindings](bindings.md) file, which lists every public member with its types.
- Oxidizer's [Rhai definitions](../rhai/editor.md), a searchable list with types.
