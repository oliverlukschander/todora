Todora 0.10.0 adds a circular mini-map, mini-sector timing, driveable runoff, persistent lap invalidity and rubber on the racing surface.

The map stays above session timing and follows the shared HUD scale. The player points upward, slightly below centre, with about 65 metres visible ahead. The next 70 metres of the route are green. Suzuka’s upper deck is drawn over the lower road with a dark outline separating the crossing.

Circuits have `ceil(length / 180 m)` equal-distance mini-sectors, clamped to 4–8. A completed sector appears for 3.5 seconds. Deltas compare the corresponding sector of the best valid lap for the active circuit and driving mode. Green means faster; red means slower. With no reference, the sector’s elapsed time appears. Restarts retain the reference, while circuit and mode changes load their own saved reference. References across sessions are interpolated from the existing ghost recording.

All four tyre contacts must leave asphalt and kerbs to invalidate a lap. “LAP INVALID” persists after rejoining, and elapsed time continues. Invalid laps cannot update the best time, reference sectors or saved ghost. Recovery also invalidates the lap. A restart or a fresh forward start restores eligibility. After a shortcut has disrupted lap progress, an invalid lap can end following at least five metres of legal forward travel on the final approach; a brief reverse crossing cannot clear invalidity.

The existing grass terrain is now driveable beyond the verge. Collision height comes from its rendered triangles, including the ground below Suzuka’s bridge. Outer-boundary and stuck-car recovery remain. Bridge edges retain their barrier. Both additional soft-ground rolling resistance and the ploughing resistance are multiplied by 0.75. Asphalt physics, grip settings, acceleration and maximum speed are unchanged.

The rubber path uses local curvature relative to the approach and exit, smoothed around the closed circuit. A soft band with slight width and density variation darkens the asphalt vertices. It stays inside the asphalt, shares the road’s elevation and leaves painted markings intact. Lighting remains unlit on circuit surfaces, without shadow maps. This is a plausible visual racing path, not a vehicle-specific lap optimiser.

Saved laps now use `TODORAL2`. Earlier recordings did not enforce four-wheel track limits, so their times and ghosts are not loaded as valid references. The files remain in place until a new valid best replaces them.

Validation on 21 September 2026: **193 tests passed, 0 failed, 8 optional report tests ignored**. Formatting and Clippy passed. The final Apple Silicon app passed strict code-signature verification and a startup check using its bundled assets. Its Mach-O UUID matches the release binary. Test and build logs are saved beside the screenshots in `dist/visual-check/`.

Validation commands:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
tools/package_macos.sh
```

Checks cover sector timing and lap seams on all 40 circuits, interpolation, reverse sector crossings, resets, circuit/mode changes, sticky invalidity, invalid ghost rejection, wheel contacts on kerbs, recovery, grass collision, bridge levels and the exact resistance reduction. Map tests check heading and continuous road strokes. Rubber checks cover every circuit’s seam, smoothness and asphalt edge fade.

The plain AI driver still has to lap every circuit. The deliberately clumsy driver now brakes to rejoin open runoff and has a bounded return angle. Its test checks travel, rejoining and time spent stopped, with a 20% allowance for the longer return from grass. Its nearest-road lap progress can jump when it shortcuts to a neighbouring straight, so that number is no longer used as proof of driveability. This AI is not connected to player controls.

Reproducible visual checks use an optional feature, excluded from the packaged app:

```sh
TODORA_CAPTURE="$PWD/dist/visual-check/suzuka-bridge.png" \
TODORA_CIRCUIT=suzuka TODORA_PROGRESS=0.78 TODORA_WIDE=1 \
cargo run --locked --features visual-check
```

The capture harness places a stationary car at a specified lap fraction. `TODORA_WIDE=1` selects an elevated inspection view. `TODORA_SMALL=1` uses an 800×600 logical window; `TODORA_PREVIEW=1` supplies an invalid-lap HUD preview. It does not drive or save a timed lap. When invoking `target/debug/todora` directly, set `BEVY_ASSET_ROOT="$PWD"` as well.

Visual review covers Monza’s first chicane, Spa’s climb, Suzuka’s upper deck and underpass, and Red Bull Ring at 800×600. Captures are in `dist/visual-check/`. These screenshots and deterministic physics checks do not replace a hands-on keyboard/gamepad session for judging feel.

The local app is `dist/Todora.app`; the packaging script also creates `dist/Todora.dmg` and signs the app ad hoc. The DMG and checksum are available in the [0.11.0 release](https://github.com/oliverlukschander/todora/releases/tag/v0.11.0).
