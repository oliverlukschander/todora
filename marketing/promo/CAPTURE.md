# Capture record — 0.12.0

Captured on 2026-10-13 from `promo/readme-0.12`, based on the local `main` checkout. All published frames are newly rendered by the 0.12.0 game, not reused from the 0.11.1 launch kit. The recorder and camera rigs were brought forward from `promo/launch-kit`; the production physics, circuit geometry and leaderboard verifier were not modified.

## Commands and environment

Apple M4 Max, macOS 27.0.1, Bevy 0.19.1, Metal. Build:

```sh
cargo build --locked --features promo
```

The first sandboxed attempt was:

```sh
python3 marketing/promo/tools/capture.py marketing/promo/shots/readme.json hero
```

It failed before rendering with:

```text
Unable to find a GPU! Make sure you have installed required drivers!
```

The sandbox prevented GPU discovery. The failed attempt's log and save folder were preserved locally as `.cache/hud-before/shots/hero-sandbox-failure.log` and `.cache/hud-before/data/hero-sandbox-failure/`. After permission to run the same capture tool outside the sandbox, Metal was detected and recording succeeded:

```sh
python3 marketing/promo/tools/capture.py marketing/promo/shots/readme.json hero title circuits awards profile accessibility audio split-screen grid corner cockpit bonnet finish ghost bonkers trackside
python3 marketing/promo/tools/capture.py marketing/promo/shots/readme.json hero-front chicken
python3 marketing/promo/tools/export.py
```

Per-shot JSON, logs, cue timings and intermediate MP4s remain in `.cache/`, ignored by git. No files under `dist/` were touched.

## What the pictures show

| Export | Actual game state |
| --- | --- |
| hero / gt | OMARCHY GT #95 at Styrian Bowl / Ardennes Loop, filmed with cinematic camera rigs |
| title | Drive and Bonkers choices, with the 51-circuit tagline |
| circuits | Endurance collection, Sarthe Run selected; All reports 51 |
| chase / bonnet / cockpit | Game cameras at Ardennes Loop / Sarthe Run / Figure-Eight Hills |
| grid | The real three-red-light countdown |
| best | An AI-driven second lap; the first was driven at at most 80% throttle |
| ghost | Racing the actual best-lap recording saved by `finish` |
| awards | Actual Awards UI, fresh capture profile, 0 of 71 |
| profile | Actual profile editor with a locally suggested name; no profile saved or submitted |
| split-screen | Four local cars, four cameras and four clocks on the grid; two synthetic pad entities exercise the normal lobby path |
| accessibility / audio | Actual settings screens |
| trackside | Cinematic view of the real GT and trackside scenery |
| bonkers / chicken | The real chicken mount, wild elevation and Bonkers effects |

No standings, sector times, best times or unlocks were invented. Menus are opened through the existing visual-check hooks. Cars are driven by the game's AI through normal physics and lap validation. Cinematic stills place the car and camera for composition; they are not screenshots of a completed player lap. Bonkers footage is separate from records and ghosts. The game always opens plain in a normal build.

The online client is disabled by `ReadOnly`. Settings and achievements are not persisted. The recorder's laps and ghost files use `.cache/data/`, never the user's usual Todora saves. Sound is disabled in the deliverable; the radio is off throughout capture.

## Review

Contact sheets and selected full-resolution frames were inspected for image content, legible UI and truthful captions. The split-screen capture's built-in assertions verify four cars, cameras, non-empty viewports and independent clocks. Exact still times and cut boundaries are in `shots/edit.json`.

The `finish` log records a valid 45.083336 s first lap, then a valid 43.76667 s second lap at game time 96.983 s. Its four sector results are `[Purple, Purple, Purple, Purple]`; the exported frame shows NEW BEST and the earned Purple reign notification.

## Final validation

- `cargo test --locked --lib`: **491 passed, 0 failed, 18 ignored**.
- `cargo clippy --locked --lib --features promo -- -D warnings`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- README and kit-document relative links resolve locally.
- README clip: **20.000 s, 1280×720, 60 fps, H.264 High, yuv420p, BT.709, silent**, 4,712,036 bytes. Its `moov` atom precedes `mdat` (`+faststart`).
- Local master: 1920×1080, same duration and delivery format, approximately 22.8 MB; ignored by git. The 18 JPGs plus compact clip total approximately 10.11 MB.

No release upload, external post or production leaderboard write was performed. The root README and compact media are intended for the repository’s `main` branch.


## HUD scaling correction

The first edit had a capture bug: both the normal UI system and the recorder
wrote `UiScale` during Update. The virtual window used for split-screen made
the normal scaler choose 1.25, while the recorder chose 1.35. Unordered systems
therefore resized the overlays between frames within a shot. This was present
in the source recordings, before the ffmpeg edit; all shots were 1920×1080.

The recorder now supplies a fixed `CaptureScale` resource. The normal UI scaler
is the only writer and honours that override. Ordinary game builds do not
include it. The regression test switches the override on and off while changing
window sizes. Before the fix, the recorded corner label's bounds varied by
`[4, 3, 16, 4]` pixels; after it, all 240 frames had identical bounds.

All deliverable footage and stills were re-rendered. The superseded footage,
logs and isolated saves remain locally in `.cache/hud-before/`. Verify the
static HUD label on every captured gameplay frame with:

```sh
python3 marketing/promo/tools/check_hud.py
cargo test --locked --lib --features promo capture_scale_stays_fixed
```

The checker uses decoded pixels, permits at most two pixels of compression
variation, and rejects the old recordings. It checks the circuit heading,
whose size and position should stay fixed while the gameplay behind it moves.

The corrected five gameplay sources pass across all **2,340 frames**: corner, cockpit, ghost and Bonkers have identical label bounds throughout; finish varies by one pixel on one edge due to compression. The before-fix source fails the same check. The focused capture-scale test, default-build check, formatting and Clippy also pass.

The final 720p edit was decoded in full. Its five gameplay segments were also checked frame by frame using a white circuit-name text region inside the HUD panel: the bounds varied by at most one pixel after compression. The 20-second file retains H.264 High, yuv420p, 60 fps, BT.709 and faststart.

## Inline README player

The corrected 4,712,036-byte compact clip was uploaded as a native GitHub attachment. GitHub’s Markdown API was checked before publication: the standalone attachment URL produces a `<video controls>` player. The repository copy remains as a download fallback; no release was published or modified.
