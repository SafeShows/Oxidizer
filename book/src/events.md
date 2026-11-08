# Events and services

Mods can talk to each other, Rust and Rhai alike, without depending on each other's code:

- **Events**: a mod publishes a value on a topic, and every mod subscribed to that topic gets it.
- **Services**: a mod provides a named function, and other mods call it and get a value back.

Oxidizer 0.9 and later (`oxidizer_api` 0.6 for Rust). The [bus_rust](https://github.com/SafeShows/Oxidizer/tree/master/examples/bus_rust) and [bus_rhai](https://github.com/SafeShows/Oxidizer/tree/master/examples/bus_rhai) examples use all of it, both ways.

## Names

Topic and service names start with the id of the mod that owns them: `weather_core.weather`, `weather_core.forecast`. Only `weather_core` can publish on `weather_core.*` or provide `weather_core.*`. Any mod can subscribe or call. That way, an event on `weather_core.weather` always comes from `weather_core`.

## Values

Values cross between mods as JSON:

- Rhai sends and receives plain values: numbers, strings, bools, arrays and object maps (`#{ kind: "Storm", strength: 3 }`).
- Rust uses its own types through serde, so a Rust mod sees typed structs and enums.

A Rhai map and a Rust struct with the same field names are the same value.

## Events

=== "Rhai"

    ```rust
    fn init() {
        subscribe("weather_core.weather", "on_weather");   // the function, by name
        publish("my_mod.ready", true);
    }

    fn on_weather(w) {
        print(`weather: ${w.kind}`);
    }
    ```

=== "Rust"

    ```rust
    use oxidizer_api::serde::{Deserialize, Serialize};
    use oxidizer_api::Event;

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(crate = "oxidizer_api::serde", tag = "kind")]
    enum Weather { Clear, Storm { strength: i64 } }
    impl Event for Weather { const TOPIC: &'static str = "weather_core.weather"; }

    // in init: keep the Subscription in your mod; dropping it unsubscribes
    let sub = oxidizer_api::subscribe(|w: Weather| oxidizer_api::log(&format!("{w:?}")))?;
    ```

Events are delivered at the start of the next frame, in the order they were published, never in the middle of the publisher's code.

**Retained events:** `publish_retained` also keeps the value as the topic's current state. A mod that subscribes later gets it right away. Use it for state ("the weather is Storm"), and `publish` for things that happen ("a storm started").

If a Rhai subscriber throws an error, that script stops, like any other Rhai error, and the other subscribers still get the event. A Rust subscriber that panics is caught and logged. An event a Rust type can't be read from is logged and skipped.

## Services

=== "Rhai"

    ```rust
    fn init() {
        provide("my_mod.add", "add");
    }

    fn add(args) {
        args.a + args.b
    }

    // elsewhere, in any mod
    let sum = call_service("my_mod.add", #{ a: 2.0, b: 3.0 });
    ```

=== "Rust"

    ```rust
    use oxidizer_api::Service;

    #[derive(Serialize, Deserialize)]
    #[serde(crate = "oxidizer_api::serde")]
    struct AddArgs { a: f64, b: f64 }
    struct Add;
    impl Service for Add {
        const NAME: &'static str = "my_mod.add";
        type Args = AddArgs;
        type Out = f64;
    }

    oxidizer_api::provide::<Add>(|args| Ok(args.a + args.b))?;
    let sum = oxidizer_api::call::<Add>(&AddArgs { a: 2.0, b: 3.0 })?;
    ```

A call runs the provider right away and returns its value. If anything goes wrong, the call fails with a message: no such service, the provider threw or returned an error, or the value doesn't fit. In Rhai it throws, so catch it with `try`/`catch`. In Rust you get an `Err(String)`.

A Rhai script can't be called while it is already running further up the same call, for example a service that calls itself, or a script that calls a service which calls back into it. That call fails with a "busy" error instead of hanging.

A provider is replaced when the same mod provides the same name again, for example from `reload()`.

## Hot reload

Subscriptions and services name a Rhai function, so they survive a reload: after you save the script, the new code gets the next event.

## Watching it

**Mods > Oxidizer > Event bus** lists the topics with their subscriber counts and retained values, the services, and the last events.
