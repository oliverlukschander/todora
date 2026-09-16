# Todora

A 3rd-person racer in [Rust](https://www.rust-lang.org/) and [Bevy](https://bevy.org/). Drive a comic shooting-brake around a scaled Red Bull Ring.

The car is the ruler (~2.4 m long, ~1.1 m wide). Spielberg is scaled to about a third of the previous game length, with an 8 m road.

The circuit is one closed spline plus one cross-section profile. Tarmac, edge lines, kerbs and grass are strips of a single loft over the same stations, so they share their edges exactly — there is no second surface to z-fight with. How wide that cross-section may be is set by the circuit itself, not by taste: see `PROFILE` in `src/track/mod.rs`.

```sh
cargo run
```

**W** and **S** are the pedals — **S** brakes while you are rolling and reverses once you have stopped. **A** and **D** steer, **Space** is the handbrake, **R** starts again — car on the grid, clock at zero, marks wiped — **Scroll** zooms. Arrow keys work too, and **Shift** still brakes.

The handling is an arcade model, not a tyre simulation: a velocity, a heading, and a grip figure that says how far the two may disagree. Steering sets how fast the car rotates; grip is how much sideways speed the tyres can scrub off each frame. Ask a corner for more than that and the car keeps rotating while its velocity does not follow — the difference is the slide, and the tyre marks. Let go and the slide pulls the nose back into line. The handbrake and a heavy right foot at the limit both pull grip down, which is where the drifts come from; braking never does, so the brakes always work however hard you are turning. Grip rises with speed, so a fast corner is a wide one rather than a lost one, and braking into a corner tightens it rather than loosening it — arriving too fast, the intuitive thing to do is the right one. Off the throttle the engine holds the car back, so a hill adds speed without running away with it. The meter top left shows speed and what the seat feels.

Needs a recent stable Rust (`rustup` on macOS). First Bevy compile is slow; later ones are not.

## Layout

| Path | What belongs there |
|---|---|
| `src/main.rs` | Binary entry. Calls `todora::run()`. |
| `src/lib.rs` | `GamePlugin` — register new feature plugins here. |
| `src/<feature>.rs` | One plugin per feature (`camera`, `world`, `lap`, `hud`, `skid`). |
| `src/car/` | `physics.rs` is the bicycle model and the only place the car's behaviour is decided; `mod.rs` wires input and the road under the wheels into it. |
| `src/track/` | `layout.rs` is the Red Bull Ring trace; `ribbon.rs` turns it into a centreline; `mod.rs` holds the cross-section and lofts it. |
| `assets/` | Runtime files Bevy loads. glTF lives in `assets/models/`. |
| `art/` | Source art. Blender files in `art/models/`. |
| `tools/` | Generators and one-off scripts. |

New gameplay goes in its own `src` module with a plugin, then gets added to `GamePlugin`. New runtime files go in `assets/`. Source meshes stay in `art/`.

Rebuild the shooting-brake from Blender:

```sh
/Applications/Blender.app/Contents/MacOS/Blender --background --python tools/make_shooting_brake.py
```
