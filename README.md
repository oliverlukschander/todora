# Todora

A 3rd-person racer in [Rust](https://www.rust-lang.org/) and [Bevy](https://bevy.org/). Drive a comic shooting-brake around a scaled Red Bull Ring.

The car is the ruler (~2.4 m long, ~1.1 m wide). Spielberg is scaled to about a third of the previous game length, with an 8 m road.

The circuit is one closed spline plus one cross-section profile. Tarmac, edge lines, kerbs and grass are strips of a single loft over the same stations, so they share their edges exactly — there is no second surface to z-fight with. How wide that cross-section may be is set by the circuit itself, not by taste: see `PROFILE` in `src/track/mod.rs`.

```sh
cargo run
```

**W** and **S** are the pedals — **S** brakes while you are rolling and reverses once you have stopped. **A** and **D** steer, **Space** is the handbrake, **R** puts you back on the racing line where you went off, **Scroll** zooms. Arrow keys work too, and **Shift** still brakes.

The car is rear-wheel drive with a weight and tyres that only have so much grip. Each axle spends one friction budget on driving and cornering together, so power on the way out of a corner costs you the corner — which is where the drifts and the tyre marks come from. The brakes take a share of whatever grip each axle has rather than a fixed force, so they stop the car at about 1.2 g on tarmac, proportion themselves under load transfer, and go soft on the grass. The g-meter top left shows what the car is pulling: sideways through a corner, up and down under power and braking and over the circuit's climbs.

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
