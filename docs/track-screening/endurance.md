# Three endurance circuits

Added to the existing browser: **Sarthe Run** (`le-mans`), **Volcano Straight**
(`fuji`) and **Orange Grove Airfield** (`sebring`). The catalogue now has 43
circuits. Searching either the invented name or the venue finds each one.
These are contemporary circuit traces, not reconstructions of the exact 2014
surfaces. Le Mans is the full 24-hour course, including both Mulsanne chicanes,
not the Bugatti circuit.

## Source and regeneration

Checked 27 September 2026. Geometry is pinned to
[tobi/track-atlas c783def5a24df](https://github.com/tobi/track-atlas/tree/c783def5a24df8ce8f034262e8228c262b854e0e),
whose underlying geometry comes from OpenStreetMap. Each archived GeoJSON in
[traces](traces) records the upstream path, full commit and original file's
SHA-256, the layout, racing direction, start/finish anchor and retrieval date.
The third coordinate is the Open-Meteo DEM elevation in metres above sea level.
Those samples are baked in, so regeneration needs no network or local cache.
[Distribution credits and data licenses](../../assets/tracks/CREDITS.md) ship
with the game assets on macOS and Linux.

| Circuit | Source | Direction |
| --- | --- | --- |
| Le Mans | `tracks/circuit-de-la-sarthe/raw/layers/24h.geojson`, OSM relation 2126739 | Clockwise |
| Fuji | `tracks/fuji/raw/layers/gp.geojson`, based on OSM way 148622740 | Clockwise |
| Sebring | `tracks/sebring/raw/layers/wec.geojson`, full International layout | Clockwise |

Fuji's outline was checked against the closed main raceway 148622740 and
[Fuji's official circuit map](https://fsw.tv/guide/facility/racing.html).
It includes the Dunlop chicane; a regression check rejects a trace that skips
its surveyed northward leg. Use the atlas's sampled outline, not the sparse
raw OSM way: the latter has too few fixes on the main straight for the
importer's nearest-fix start anchor. The sampled outline places the line
correctly and retains the DEM's relief along the straight.

Sebring uses the atlas's refined midline; WEC and IMSA share its main lap.
Pit-lane configurations are not modelled in Todora. Compare the layout with
[the operator's Super Sebring map](https://www.sebringraceway.com/wp-content/uploads/sites/1019/2022/07/22/track-map.pdf).
Le Mans follows the full circuit shown in
[the organiser's map](https://www.24h-lemans.com/en/info/plan-circuit).
Surveyed centreline lengths can differ from official racing lengths; no
coordinates are stretched to force a published number.

From the repository root:

```sh
python3 tools/make_track.py docs/track-screening/traces/le-mans.geojson le_mans "Sarthe Run"
python3 tools/make_track.py docs/track-screening/traces/fuji.geojson fuji "Volcano Straight"
python3 tools/make_track.py docs/track-screening/traces/sebring.geojson sebring "Orange Grove Airfield"
```

As with existing imports, the generator sets `lap` to zero. Measure the
finished road with `cargo test --locked --lib the_laps -- --ignored --nocapture`
and record its menu length. Keep any admission scale adjustment documented
below. The shared road width, physics and existing circuit coordinates remain
unchanged. The DEM models ground relief, not detailed surface bumps or banking.

## Compatibility

The weekly challenge deliberately retains its original, explicitly pinned
40-circuit roster. Adding data must not reshuffle old weeks or put a new client
on a different circuit from an older server. Introducing new tracks into weekly
challenges needs a separately coordinated roster/version change.

Existing saved ghosts retain their circuit IDs and geometry fingerprints.
The `circuits-40` and `gold-40` awards remain 40-circuit milestones, with wording
that no longer claims they cover the entire catalogue. Earned awards are kept.
Each new circuit has medal targets for Beginner, Regular and Pro; AI-derived
targets are marked provisional, as elsewhere in the game.

The leaderboard server must be rebuilt from the same source to accept runs on
the three new IDs. Existing circuits continue to use the same physics and
fingerprints. This change does not deploy the server.

## Admission measurements

All three retain the shared `plan_scale = 1` and `corners = 1`; no existing
layout, road width or physics tuning was changed.

| Circuit | Source samples | Projected source lap | Finished lap | DEM range |
| --- | ---: | ---: | ---: | ---: |
| Sarthe Run | 1,200 | 13.627 km | 1,617.8 m | 44–84 m |
| Volcano Straight | 315 | 4.559 km | 523.7 m | 547–604 m |
| Orange Grove Airfield | 835 | 5.848 km | 686.4 m | 17–22 m |

DEM ranges describe the sampled ground model, not a claim about surveyed road
heights. The common smoothing and grade cap handle its coarse steps. Fuji is
included among the camera test's hilly circuits; it must still keep the level
camera for more than five sixths of the lap, and the full visibility test must
pass at every tested zoom and speed.

The original forty rows of `src/car/determinism.txt` remain byte-for-byte
identical. Only three rows are added. Existing author medal targets are also
unchanged; new targets are provisional AI laps in all three modes.

Validation on macOS / Apple Silicon:

- `cargo test --locked --workspace`: 327 game tests and 15 server tests pass;
  12 optional reports remain ignored.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`: pass.
- `python3 -m unittest discover -s tools -p 'test_*.py'`: 8 tests pass, including
  offline reproduction of every imported coordinate/elevation and the Fuji
  chicane regression.
- `cargo fmt --all --check`: pass.

The standard circuit tests cover shape retention, fitted road/mesh geometry,
relief and grade caps, racing direction, run-up and start-line speed, lap and
sector timing, AI drivers, car balance, scenery placement and camera visibility.
The new replay fingerprints were generated on this Mac; Linux CI checks the
same fixture after publication.

Native startup checks loaded all three circuits and assets without errors.
The macOS screenshot harness returned all-black images, including the existing
Red Bull Ring control. Native visual review is therefore **inconclusive**,
not a pass.
`tools/check_linux.sh` now captures these three circuits as well as Suzuka,
Monza and Monaco, and rejects blank images and asset/render errors in CI.
