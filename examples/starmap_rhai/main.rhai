// Talks with a C# StarMap mod (examples/starmap_cs) through StarMap's event bus. Needs Oxidizer.StarMap.
// StarMap matches messages by C# type, so they are named "Namespace.Type, Assembly".

fn init() {
    if !starmap_available() {
        print("starmap_rhai: StarMap's event bus isn't here (run Oxidizer as Oxidizer.StarMap)");
        return;
    }
    // Typed by name: the message is the C# object.
    starmap_subscribe("StarTest.Ping, StarTest", "on_ping");
    // Or mirrored onto Oxidizer's own bus, as a map.
    let topic = starmap_forward("StarTest.Ping, StarTest");
    subscribe(topic, "on_forwarded");
}

fn on_ping(ping) {
    let n = ping.get("Count").float().to_int();
    print(`starmap_rhai: got ping ${n}`);
    // A map whose keys are the C# type's properties becomes that type.
    starmap_publish("StarTest.Pong, StarTest", #{ From: "Rhai", Count: n });
}

fn on_forwarded(ping) {
    print(`starmap_rhai: forwarded ping ${ping.Count}`);
}
