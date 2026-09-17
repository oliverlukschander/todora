# Todora

A 3rd-person racer in [Rust](https://www.rust-lang.org/) and [Bevy](https://bevy.org/). Drive a comic shooting-brake around a scaled Red Bull Ring.

The car is the ruler (~2.4 m long, ~1.1 m wide). Spielberg is scaled to about a third of the previous game length, with an 8 m road.

The circuit is one closed spline plus one cross-section profile. Tarmac, edge lines, kerbs and grass are strips of a single loft over the same stations, so they share their edges exactly — there is no second surface to z-fight with. How wide that cross-section may be is set by the circuit itself, not by taste: see `PROFILE` in `src/track/mod.rs`.

```sh
cargo run
```

**W** and **S** are the pedals — **S** brakes while you are rolling and reverses once you have stopped. **A** and **D** steer, **Space** is the handbrake, **R** starts again — car on the grid, clock at zero, marks wiped — **Scroll** zooms. Arrow keys work too, and **Shift** still brakes. A gamepad works alongside: left stick steers, the triggers are the pedals, the south button is the handbrake and Start restarts. The stick and triggers are analogue, and on this car that is the difference between full lock and the lock you meant.

The handling is an arcade model, not a tyre simulation: a velocity, a heading, and a grip figure that says how far the two may disagree. Steering sets how fast the car rotates; grip is how much sideways speed the tyres can scrub off each frame. Ask a corner for more than that and the car keeps rotating while its velocity does not follow — the difference is the slide, and the tyre marks. Let go and the slide pulls the nose back into line. The handbrake and a heavy right foot at the limit both pull grip down, which is where the drifts come from; braking never does, so the brakes always work however hard you are turning. Grip rises with speed, so a fast corner is a wide one rather than a lost one, and braking into a corner tightens it rather than loosening it — arriving too fast, the intuitive thing to do is the right one. Off the throttle the engine holds the car back hard — about half of what the brakes do — so the throttle is the speed control and the brakes are for the big stops: lift for a corner and the car settles into it. It also means a hill only gives speed while you are on the power. The grass is a gravel trap: at pace it costs most of a g and the wheels spin, so going off ends the corner, though a car can always crawl back to the road. The meter top left shows speed and what the seat feels. The engine steps at a fixed 240 Hz whatever the frame rate, so a slow machine and a fast one drive the same car, and everything a car is lives in one `Handling` value, so a second car is data rather than code.

Needs a recent stable Rust (`rustup` on macOS). First Bevy compile is slow; later ones are not.

## Layout

| Path | What belongs there |
|---|---|
| `src/main.rs` | Binary entry. Calls `todora::run()`. |
| `src/lib.rs` | `GamePlugin` — register new feature plugins here. |
| `src/<feature>.rs` | One plugin per feature (`input`, `camera`, `world`, `lap`, `hud`, `skid`). |
| `src/car/` | `physics.rs` is the engine: a pure `step` over a `Car`, its `Handling` (everything that makes one car drive like itself, as a value) and a `Surface`. `mod.rs` is the entity — the leaning body, the wheels, and `advance`, which carries `Controls` through the engine in fixed substeps. `driver.rs` is the AI: it laps the circuit in the tests today and drives opponents tomorrow. |
| `src/track/` | `layout.rs` is the Red Bull Ring trace; `ribbon.rs` turns it into a centreline; `profile.rs` is the cross-section — the mesh, the height under a wheel and the grip under it all read from one table; `mod.rs` is the `Track` the game asks where the ground is. |
| `assets/` | Runtime files Bevy loads. glTF lives in `assets/models/`. |
| `art/` | Source art. Blender files in `art/models/`. |
| `tools/` | Generators and one-off scripts. |

New gameplay goes in its own `src` module with a plugin, then gets added to `GamePlugin`. New runtime files go in `assets/`. Source meshes stay in `art/`.

## A Mac app

```sh
tools/package_macos.sh
```

Builds a release binary, wraps it in `dist/Todora.app` with its icon, and puts that in `dist/Todora.dmg` next to an Applications shortcut. It uses only what macOS ships with: QuickLook renders the icon from `art/icon/todora.svg`, `iconutil` packs it, `hdiutil` makes the disk image. The app is signed ad hoc, which is all this Mac needs; another Mac will ask for right-click → Open the first time, because there is no Developer ID behind it.

Rebuild the shooting-brake from Blender:

```sh
/Applications/Blender.app/Contents/MacOS/Blender --background --python tools/make_shooting_brake.py
```
