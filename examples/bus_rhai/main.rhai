// bus_rhai: the Rhai half of the event bus example; bus_rust is the Rust half.
// Events and services carry plain values (numbers, strings, bools, arrays, maps); names start with this mod's id.

fn init() {
    this.frames = 0;
    provide("bus_rhai.greet", "greet");
    provide("bus_rhai.recurse", "recurse");
    subscribe("bus_rust.tick", "on_tick");
    // Retained: bus_rust (or any mod that subscribes later) gets the current weather right away.
    publish_retained("bus_rhai.weather", #{ kind: "Storm", strength: 3 });
}

fn loaded() {
    print(`bus_rhai: bus_rust says 2 + 3 = ${call_service("bus_rust.add", #{ a: 2.0, b: 3.0 })}`);
    // A script can't call into itself while it runs: the call fails as "busy" instead of hanging.
    try {
        call_service("bus_rhai.recurse");
    } catch (e) {
        print(`bus_rhai: refused, as it should be: ${e}`);
    }
}

// Services get the caller's value and return theirs.
fn greet(args) {
    #{ text: `hello ${args.name}, from Rhai` }
}

fn recurse(args) {
    call_service("bus_rhai.recurse")
}

fn on_tick(t) {
    print(`bus_rhai: tick at frame ${t.frame}`);
}

fn frame(t, dt) {
    this.frames += 1;
    if this.frames == 600 {
        publish_retained("bus_rhai.weather", #{ kind: "Clear" });
    }
}
