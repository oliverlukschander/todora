# Todora

A 3rd-person racer in [Rust](https://www.rust-lang.org/) and [Bevy](https://bevy.org/). Drive a comic shooting-brake around a scaled Red Bull Ring.

The car is the ruler (~2.4 m long, ~1.1 m wide). Spielberg is scaled to about a third of the previous game length, with an 8 m road.

The circuit is one closed spline plus one cross-section profile. Tarmac, edge lines, kerbs and grass are strips of a single loft over the same stations, so they share their edges exactly — there is no second surface to z-fight with. How wide that cross-section may be is set by the circuit itself, not by taste: see `PROFILE` in `src/track/mod.rs`.

```sh
cargo run
```

**WASD** or **arrows** to drive. **Shift** to brake. **Scroll** to zoom.

Needs a recent stable Rust (`rustup` on macOS). First Bevy compile is slow; later ones are not.

## Layout

| Path | What belongs there |
|---|---|
| `src/main.rs` | Binary entry. Calls `todora::run()`. |
| `src/lib.rs` | `GamePlugin` — register new feature plugins here. |
| `src/<feature>.rs` | One plugin per feature (`car`, `camera`, `track`, `world`, `lap`, `hud`). |
| `src/track/` | `layout.rs` is the Red Bull Ring trace; `ribbon.rs` turns it into a centreline; `mod.rs` holds the cross-section and lofts it. |
| `assets/` | Runtime files Bevy loads. glTF lives in `assets/models/`. |
| `art/` | Source art. Blender files in `art/models/`. |
| `tools/` | Generators and one-off scripts. |

New gameplay goes in its own `src` module with a plugin, then gets added to `GamePlugin`. New runtime files go in `assets/`. Source meshes stay in `art/`.

Rebuild the shooting-brake from Blender:

```sh
/Applications/Blender.app/Contents/MacOS/Blender --background --python tools/make_shooting_brake.py
```
