# Your first Rhai mod

A Rhai mod is a folder with two files. Make `mods/my_mod/` with this `mod.toml`:

```toml
name = "my_mod"

[oxidizer]
script = "main.rhai"
```

and this `main.rhai`:

```rust
fn init() {
    print("my_mod: hello!");
}

fn gui(dt) {
    if begin("My mod") {
        let v = ksa::Program::controlled_vehicle();
        if v == () {
            text("no vehicle");
        } else {
            text(`flying ${v.id} at ${v.get_barometric_altitude().to_int()} m`);
        }
    }
    end();
}
```

Start the game (or, if it is already running, wait a second: Oxidizer starts new Rhai mods by itself). A window called **My mod** shows the vehicle you fly, and `my_mod: hello!` is in **Mods > Oxidizer**'s log.

Now change the text in `main.rhai` and save the file. Within a second the window shows your change, without restarting anything. That loop, save and look, is how Rhai mods are written.

## Three ways to start

- **By hand**, as above.
- **[Rhai Studio](https://github.com/SafeShows/oxidizer-rhai-studio)**, an editor inside the game: type a name, press CREATE, and a starter mod is running; edit it with syntax colors and save with Ctrl+S.
- **The [Rhai mod template](https://github.com/SafeShows/oxidizer-rhai-mod-template)** on GitHub: a repository with a starter mod, a release workflow and a content index listing ready to fill in. Clone it straight into the mods folder and edit it with your editor.

## Rhai in one minute

Rhai reads like Rust without the types:

```rust
let speed = 12.5;                 // variables
let name = "Gemini";              // strings; `...${x}...` in backticks interpolates
let parts = [1, 2, 3];            // arrays
let info = #{ id: 1, ok: true };  // object maps

if speed > 10.0 { print("fast"); } else { print("slow"); }
for p in parts { print(p); }

fn twice(x) { x * 2 }             // the last expression is the return value
```

Integers and floats are different types: `1 + 1.0` is an error, write `1.0 + 1.0` or `to_float(1)`. The [Rhai book](https://rhai.rs/book/) has the whole language.
