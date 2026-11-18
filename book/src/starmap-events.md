# StarMap's event bus

When Oxidizer runs as the StarMap mod **Oxidizer.StarMap**, Rust and Rhai mods can talk with StarMap's C# mods through
StarMap's own event bus (`IEventBus`). Oxidizer 0.13 and later, with StarMap 0.5.2 or later; `oxidizer_api` 0.8 for
Rust.

StarMap matches messages by their C# type, so a mod names the type, as `"Namespace.Type, Assembly"`. A C# mod with

```csharp
namespace StarTest;
public sealed record Ping(int Count);
```

in its assembly `StarTest` sends `"StarTest.Ping, StarTest"`. StarMap's named channels are an optional extra
argument; without one, the default channel is used.

## Receiving messages

=== "Rhai"

    ```rust
    fn init() {
        starmap_subscribe("StarTest.Ping, StarTest", "on_ping");
    }

    fn on_ping(ping) {
        // The C# object: read its properties by name.
        print(`ping ${ping.get("Count")}`);
    }
    ```

=== "Rust"

    ```rust
    use oxidizer_api::starmap;

    #[derive(serde::Deserialize)]
    #[serde(crate = "oxidizer_api::serde")]
    struct Ping {
        #[serde(rename = "Count")]
        count: i64,
    }

    // in init: keep the Subscription in your mod; dropping it unsubscribes
    let sub = starmap::subscribe_as("StarTest.Ping, StarTest", "", |p: Ping| log(&format!("ping {}", p.count)))?;
    ```

    `starmap::subscribe` gives the message as a game object (`Obj`) and its JSON instead.

Messages arrive at the start of the next frame, on the game's thread, like Oxidizer's own events. (StarMap calls
its subscribers on the thread that published; Oxidizer queues them for its mods.)

A StarMap mod can load after Oxidizer.StarMap, so its types may not exist yet when your mod starts. A subscription to
a type that isn't loaded waits for it and attaches as soon as the mod's assembly loads. If the type still isn't
there once the game has loaded, the log says so.

## Sending messages

=== "Rhai"

    ```rust
    // A map whose keys are the C# type's properties (case doesn't matter) becomes that type.
    starmap_publish("StarTest.Pong, StarTest", #{ From: "Rhai", Count: 3 });
    ```

=== "Rust"

    ```rust
    #[derive(serde::Serialize)]
    #[serde(crate = "oxidizer_api::serde")]
    struct Pong<'a> {
        #[serde(rename = "From")]
        from: &'a str,
        #[serde(rename = "Count")]
        count: i64,
    }

    starmap::publish("StarTest.Pong, StarTest", "", &Pong { from: "Rust", count: 3 })?;
    ```

The value is turned into the C# type with `System.Text.Json`, so records with constructors work. A game object that
already is of the type can be sent as it is: `starmap_publish(type, obj)` in Rhai, `starmap::publish_obj` in Rust.

## On Oxidizer's bus

`starmap_forward(type)` (Rust: `starmap::forward`) copies a type's messages onto Oxidizer's own
[event bus](events.md) as JSON, and returns the topic, `starmap.<Namespace.Type>` (plus `/<channel>` for a named
channel). Then `subscribe` works on it like on any Oxidizer event, and Rhai gets the message as a map:

```rust
let topic = starmap_forward("StarTest.Ping, StarTest");   // "starmap.StarTest.Ping"
subscribe(topic, "on_ping");                               // on_ping(ping) gets #{ Count: 3 }
```

## Without StarMap

Standalone Oxidizer has no StarMap: every function returns an error saying so, and `starmap_available()` is
`false`. Check it if your mod also runs standalone.

## Example

[`examples/starmap_cs`](https://github.com/SafeShows/Oxidizer/tree/master/examples/starmap_cs) is a C# StarMap mod
that sends a `Ping` every 3 seconds and logs each `Pong`.
[`starmap_rhai`](https://github.com/SafeShows/Oxidizer/tree/master/examples/starmap_rhai) and
[`starmap_rust`](https://github.com/SafeShows/Oxidizer/tree/master/examples/starmap_rust) answer each ping:

```
[StarTest] ping 1
[oxidizer] starmap_rhai: got ping 1
[StarTest] pong 1 from Rhai
[oxidizer] starmap_rust: got ping 1
[StarTest] pong 1 from Rust
[oxidizer] starmap_rhai: forwarded ping 1
```
