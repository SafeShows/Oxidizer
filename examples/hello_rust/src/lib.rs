use oxidizer_api::{Mod, Obj, imgui, log};

struct Hello {
    visible: bool,
    show: bool,
    frames: u64,
}

impl Mod for Hello {
    fn init() -> Self {
        log("hello_rust: init");
        // Postfix on an instance method; args[1] is the GlfwFocusEvent.
        oxidizer_api::hook("KSA.Program", "OnFocus", true, |_inst, args| {
            log(&format!("hello_rust: focus event {}", args.index(1)));
            true
        });
        Hello { visible: true, show: true, frames: 0 }
    }

    fn frame(&mut self, _t: f64, _dt: f64) {
        self.frames += 1;
    }

    // Entries in the game's "Mods" menu.
    fn menu(&mut self) {
        if imgui::menu_item("Hello Rust", self.visible) {
            self.visible = !self.visible;
        }
    }

    fn gui(&mut self, _dt: f64) {
        if !self.visible {
            return;
        }
        if imgui::begin("Hello Rust") {
            let vehicle = Obj::static_get("KSA.Program, KSA", "ControlledVehicle");
            let id = if vehicle.is_null() { "none".into() } else { vehicle.get("Id").to_string() };
            let secs = Obj::static_call("KSA.Universe, KSA", "GetElapsedSeconds", &[]).f64().unwrap_or(0.0);
            imgui::text(&format!("frames: {}", self.frames));
            imgui::text(&format!("vehicle: {id}"));
            imgui::text(&format!("elapsed: {secs:.1} s"));
            imgui::checkbox("show speed", &mut self.show);
            if self.show {
                let speed = Obj::static_get("KSA.Universe, KSA", "SimulationSpeed");
                imgui::text(&format!("sim speed: {speed}"));
            }
            if imgui::button("x1") {
                Obj::static_call("KSA.Universe, KSA", "SetSimulationSpeed", &[&1.0.into(), &Obj::from(1.0)]);
            }
            imgui::same_line();
            if imgui::button("x10") {
                Obj::static_call("KSA.Universe, KSA", "SetSimulationSpeed", &[&10.0.into(), &Obj::from(1.0)]);
            }
        }
        imgui::end();
    }
}

oxidizer_api::oxidizer_mod!(Hello);
