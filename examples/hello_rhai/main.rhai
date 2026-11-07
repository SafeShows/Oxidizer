// Per-mod state lives in `this`.
fn init() {
    print("hello_rhai: init");
    this.frames = 0;
    this.speed = 1.0;
    // Prefix hook: return false to skip the original method.
    patch("KSA.Program", "OnFocus", "prefix", "on_focus");
}

fn on_focus(program, args) {
    print(`hello_rhai: focus ${args[1]}`);
    true
}

fn frame(t, dt) {
    this.frames += 1;
}

fn gui(dt) {
    if begin("Hello Rhai") {
        let v = static_get("KSA.Program, KSA", "ControlledVehicle");
        text(`frames: ${this.frames}`);
        text(`vehicle: ${if v.is_null() { "none" } else { v.get("Id").to_string() }}`);
        text(`elapsed: ${static_call("KSA.Universe, KSA", "GetElapsedSeconds").float()}`);
        this.speed = slider("warp", this.speed, 1.0, 100.0);
        if button("apply") {
            static_call("KSA.Universe, KSA", "SetSimulationSpeed", [this.speed, true]);
        }
    }
    end();
}
