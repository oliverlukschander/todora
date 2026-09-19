Todora should admit all 40 source layouts through the same geometry and driving contracts. Do the lookup work first, then variable verges, then road proportions, then bridges. Keep the 75% retention bar. Each admission must pass the whole suite, not just construction.

This is a researched implementation plan for the maintainer, based on Todora `3c1421e` and bacinger/f1-circuits `394d8fbe70ef2c0b0c8d23ff7bee61fa09606055`, checked on 2026-09-19. The engine has not been changed. The screening utility and results accompany this plan so the experiments can be repeated.

**What the investigation established**

The unchanged checkout passes `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` and `cargo test --locked --all-targets`: 117 passed, four ignored, 39.08 seconds. The requested `the_circuits` report also passes. README, track/profile/ribbon/marker documentation, the car and driver modules, lap timing, ghost persistence, and the history of the two example property corrections were read before selecting the changes.

The screening uses the generator's projection and centimetre rounding, the actual Rust ribbon implementation, existing corner factors for registered tracks, and `corners = 1` for candidates. Heights are zero because these experiments measure plan geometry. They establish neither DEM suitability nor playability. In particular, a candidate passing this screen still needs imported elevation, loft validation, markers and driver tests.

The six width failures reproduce closely:

| Source layout | Available half-envelope at today's 10 m radius target |
|---|---:|
| Barcelona | 5.378 m |
| Madrid | 5.025 m |
| Jacarepaguá / Nelson Piquet | 5.138 m |
| Baku | 1.267 m |
| Sochi | 3.510 m |
| Interlagos | 4.392 m |

There are two corrections to the supplied inventory. The [source's `br-1977`](https://github.com/bacinger/f1-circuits/blob/394d8fbe70ef2c0b0c8d23ff7bee61fa09606055/circuits/br-1977.geojson) is Jacarepaguá in Rio, not Brasília. Also, proper segment-intersection testing finds a crossing in Suzuka's raw trace but none in [Nürburgring's `de-1927`](https://github.com/bacinger/f1-circuits/blob/394d8fbe70ef2c0b0c8d23ff7bee61fa09606055/circuits/de-1927.geojson). At today's settings Nürburgring retains 72.27% and has 10.28 m minimum nonlocal station separation; at a 5 m target it retains 83.72%. Do not manufacture a bridge there to match the old category. Verify the finished geometry and layout against the [operator's circuit information](https://nuerburgring.de/info/nuerburgring/race-tracks/grand-prix-track) during admission. Raw proper-intersection testing alone does not rule out touches or processing artifacts.

The fresh primary grouping is therefore 17 plan passes, 16 retention failures including Nürburgring, six width failures and one verified raw crossing. The original 15 retention failures are Yas Marina, Buenos Aires, Shanghai, Hockenheim, Magny-Cours, Hungaroring, Mugello, Monaco, Sepang, Zandvoort, Estoril, Algarve, Jeddah, Miami and Kyalami. Preserve *all* failing measurements: Zandvoort also has only 2.753 m half-envelope, and Jeddah and Miami do not converge to the current curvature target. Monaco shrinks below the 120 m needed for the existing 60 m separation exclusion to compare any pair at all; its separation result is missing, not evidence of unlimited room.

**The design decision**

Use one narrower road for every circuit, initially prototype 3.3 m total width, including kerbs and paint. Keep the car and handling units fixed. Of the two per-circuit exceptions, choose **plan scale**, with a documented multiplier only where the shared road cannot fit. Reject per-circuit road width: the difficult circuit should not also get a different amount of steering room, and Baku would force a road substantially narrower than two car widths before allowing any verge.

This explicitly gives up the README's promise that relative game-lap lengths preserve relative real-lap lengths. Seconds, metres and car handling remain common; road width remains common; cross-circuit length ratios do not. Display the finished lap length in the circuit menu and document the scale policy and every exception. Never normalize records by the multiplier: cornering and acceleration do not scale linearly. Keep the same global height scale initially, then validate hills on every scaled candidate.

Why the exception is necessary: with a 5 m radius target, every raw layout retains over 75%, with Monaco lowest at 81.49%. But Baku still passes within 2.457 m of itself, so even a 3.3 m road without grass cannot fit. Monaco and Miami have only 1.987 m and 1.949 m half-envelopes. A common 3.3 m road plus a provisional 0.6 m verge needs 2.25 m per side. Plan-only probes give these feasible starting points:

| Layout | Multiplier on `0.4 / 3` | Finished lap | Retained | Available half-envelope |
|---|---:|---:|---:|---:|
| Baku | 2.00 | 1,572 m | 99.99% | 2.295 m |
| Monaco | 1.25 | 444 m | 84.20% | 2.427 m |
| Miami | 1.25 | 877 m | 99.20% | 2.312 m |

Those margins are small and use the old station-separation proxy. They are experiment seeds, not certified final values. Baku becoming longer than Spa is a real cost of the choice. The alternative that preserves length ratios is increasing the *global* plan scale for all 40; that makes every existing race substantially longer. Prefer the per-circuit scale exception over that game-wide expansion, and document the changed promise before shipping it.

**0. Prerequisite: exact indexed location and reproducible screening**

Build an immutable bounding-volume hierarchy over centreline segments when constructing `Ribbon`. Use bounds to prune candidates, then the existing segment projection and interpolation to produce `Fix`. Include the closing segment. Preserve tie-breaking by station index so indexed and exhaustive searches agree. A last-segment hint can supply an initial candidate, but must not limit correctness: resets, distant probes, reverse travel and circuit changes need a global search. Start with the index; a cached car cursor is unnecessary for the initial speedup.

Keep the exhaustive implementation as a test oracle. Compare the entire fix over stations, seam points, road edges, random off-track positions and far-away queries on every circuit. Include equal-distance fixtures. Benchmark identical deterministic query batches before and after, plus the driver harness and full suite. Record candidate projections and timings; require fewer projections and a measured speedup without a flaky wall-clock threshold in CI. Index construction and eventual 40-track loading also need timing.

There is another scaling cost: `min_separation` is quadratic and many tests rebuild every track. Reuse spatial bounds for fitting where useful, but do not hide geometry regressions behind a new global track cache. Correctness and the measured lookup improvement come first.

Extend the screen during implementation to report finished segment crossings, convergence, local envelopes, elevation failures and driver failures independently. Pin the trace revision and cache the imported DEM samples; tests must run offline. Store source ids and provenance alongside generated modules. A failed candidate belongs in diagnostic inputs until it passes admission.

**1. Variable verges: retain the 8 m road for this commit**

Fit one cross-section at each station, keeping the road ribs fixed and allowing left and right verges to differ. Start from the nominal section and impose curvature and nearby-road constraints. Taper narrower sections into their neighbours with a bounded rate of change, always reducing widths to smooth them; averaging must never widen a constrained station beyond its clearance.

Replace the universal 1.5 m minimum with physical constraints: room for the car's held footprint, a usable shoulder, the complete marker footprint and numerical clearance. This is a documented correction: 1.5 m measured generous runoff everywhere, rather than whether the road and its shoulder actually fit. Merely storing the same 1.5 m minimum per station admits none of the six failures. Barcelona, Madrid and Jacarepaguá are the first targets; Baku and Sochi cannot hold the current road, and Interlagos needs the second stage for a useful shoulder.

Store fitted ribs once. `Profile::height`, the mesh, local edge/wall bounds and markers must read that same station/segment data. Give the sampling result a segment id and interpolation fraction rather than independently re-locating every consumer. Keep surface grip derived from band boundaries: with road ribs fixed, its values may still depend only on lateral offset; variable verge width alone does not change the asphalt/kerb grip thresholds.

Interpolate between stations explicitly and test surface height against the actual triangulated quads, including samples inside changing-width bands. A bilinear profile interpolation is not automatically the plane of either rendered triangle. Account for longitudinal as well as lateral surface slope on tapered verges. Compressing the old 0.95 m vertical drop into a narrow shoulder also makes the verge and diamonds too steep; fit vertical falls to the allowed slopes instead of keeping every old rib height.

Replace `markers::LATERAL` and its nominal-edge const assertions with positions derived from each local shoulder. Check all four corners of each diamond against the fitted surface, including their forward/backward offsets. Keep their size, palette, paired sides and longitudinal rhythm. Do not omit markers to make a circuit fit. `Track::hold` uses the local edge after movement and after correction; rescue uses the same fix.

The loft's guarantee has two parts. Sharing each fitted station ring preserves exact seams between bands. It does **not** prove that distant parts of the mesh cannot intersect. In the continuous sweep, ordered ribs and `1 - curvature * lateral > margin` still prevent local inversion; a changing width does not by itself invalidate that condition. The discrete triangles need their own winding and degeneracy checks, and non-neighbouring strips need clearance/intersection checks over whole segments, not just station points. Remove the blanket 60 m exclusion as a proof of safety: it can miss separate turns inside that arc distance. Use mesh adjacency to distinguish shared seams from unwanted intersections.

Exit: all 17 existing tracks plus each admitted candidate pass the existing properties, and new properties cover narrow shoulders, rapid transitions, left/right asymmetry, seam closure, local wall containment, marker footprints and nonlocal overlap. Update README and module prose to describe exactly what has been proved. Expected first additions: Barcelona, Madrid and Jacarepaguá; their elevation and driver gates remain to be checked.

**2. Road proportions, retained identity and the remaining planar circuits**

Prototype the common 3.3 m section, with individually designed asphalt, line and kerb bands; do not multiply every old dimension blindly. Keep enough usable asphalt for the actual tyre footprint. A 5 m radius target is supported by the screening and is the initial driving candidate. Derive the admissible local offsets from curvature rather than retaining an unexplained coupling to a 7 m envelope. Tighter-radius experiments at 4 m and 3 m retain more geometry, but require separate steering and braking evidence before being selected.

Add explicit `Circuit::plan_scale` data with a shared default and recorded exceptions. Find the smallest reviewed multipliers that clear the actual fitted mesh and driving tests, rather than targeting a lap length. Re-evaluate Monza's 3.5 and Spa's 2.0 corner factors: the wide road motivated them, and retaining them by habit would preserve unnecessary distortion. Keep a before/after plan overlay and named corner evidence for every circuit whose shaping changes.

Do not lower `LEAST_KEPT` to 0.70. Losing 29% is not sufficient evidence that the layout remains recognisable, and the narrower-road experiments make that concession unnecessary. Keep retention as a guard and add what it misses: source-versus-finished displacement, turn order, preservation of identified chicanes/hairpins and unintended crossings. Set tolerances against source resolution and the intended road envelope, with fixtures that fail when a named feature is erased. Retention currently starts *after* the spline and exaggeration; it cannot detect corners those stages already removed.

Change the plain driver's lookahead to sample by distance along the ribbon. Its current straight-ahead world-space probes can select a neighbouring straight, and at a bridge can select the other deck. Then assess the narrower road with both driver styles, all three cars and all three setups. Preserve the clumsy driver's reaction/late-braking premise. Fix steering or road geometry when necessary; do not relax time, off-road, stopped-time or garage-balance bars to admit a track. Re-measure braking-marker coverage: lowering the minimum corner speed makes the hardest stop longer, so the old 16 m explanation must earn its place again. Update the common marker rhythm coherently if it must change.

Extend `Track::fingerprint` before any changed geometry ships. Hash the finished driving surface, including cross-section widths/heights, grip boundaries and effective bridge geometry, plus a compatibility revision for changes to lap interpretation. The current position-only centreline hash misses a width-only change. Test that width-only and height changes invalidate old records, stable surfaces retain their hash, and visual paint changes need not invalidate laps. Existing files should be rejected normally and remain on disk until replaced by a new best; no conversion or deletion of saved ghosts is required. State the reset of compatible best times in the release notes.

Exit: every non-crossing source layout passes every existing property and the new identity/mesh tests. Subject to the Nürburgring audit, that is 39 tracks. Re-run the garage report and document new lap times; all cars still need a reason to be chosen. Recheck run-up placement and line speed, marker coverage, menu navigation with 40 rows, frame-rate independence, reset/switch behaviour and ghost persistence. Play keyboard and gamepad laps on Baku, Monaco and a fast existing circuit before calling the new proportions finished.

**3. A bridge feature, then Suzuka and any other verified crossing**

Scope the feature to a grounded car on vertically separated sections of one continuous circuit. It includes deck choice, ramps, visible bridge structure, clearance, progress, respawn and camera traversal. General jumps, falling between decks and a full rigid-body collision engine are outside this feature.

Represent each verified crossing with source-anchored upper/lower intervals, which interval passes over, a ramp influence interval, clearance and provenance. Anchor to source segment/fraction or surveyed landmarks, not finished station numbers that change after smoothing. Prefer an operator map plus surveyed/profile or structure measurements. The 90 m DEM is ground elevation, not road-deck evidence. Where measured heights cannot be obtained, document an authored gameplay clearance and its source-informed topology; never label an invented height as DEM data. Exact bridge elevations remain unresolved in this planning pass.

Solve the bridge heights after ordinary DEM smoothing, with constraints that survive grade limiting. A later blanket smoothing/capping pass must not flatten the bridge. Check separation over the entire overlapping road footprints, including car height, deck thickness and margin, rather than only at the centreline intersection. Ramp lengths follow from the required rise and grade backstop; also test smooth slope changes and retained local relief. An infeasible ramp is a construction failure, not permission to exceed the cap.

Make the spatial index return nearby candidate segments. Resolve the drivable deck using surface height plus continuity from the car's previous segment/arc distance. Height alone can still choose incorrectly on steep ramps or when the query is off the centreline. Seed on the grid, re-seed on a circuit switch or explicit teleport, and keep the resolved segment through wall correction, rescue, wheel/skid samples, lap timing and ghost progress. Arbitrary queries without history need a deterministic documented policy. Arc-distance AI lookahead follows its selected road through the crossing.

Update `Track::ground`, `hold`, `rescue`, `progress`, `start_along`, `on_start_gate`, the lap gate and all direct `Ribbon::locate` users together. Gate a lap by local road interval and deck as well as forward plane crossing. Driving under the start plane must not start or finish a lap; reversing and seam wrapping must retain their current meaning. Persisted ghost poses already carry height, but progress and compatibility must use the new surface identity.

Loft the upper and lower road through the same profile rules. Add an underside/support treatment so the crossing reads as a bridge and the car does not see through a one-sided road. Keep structural geometry separate from the upward-facing drivable-surface contract. All road and marker triangles remain subject to `loft_faces_up`; explicit bridge side/underside faces receive appropriate winding/clearance tests instead of weakening that property. Update “nothing overlaps” to mean no unintended 3D surface intersections: intended plan overlap at a bridge has verified vertical clearance.

First prove a small synthetic figure-eight in both directions, with a crossing near the start gate, overlapping grass, height noise, resets, rescue and off-centre wheel samples. Then admit Suzuka using audited bridge metadata. Require zero deck switches at crossings, continuous progress with one wrap per lap, one legitimate start gate, no false lap on the lower road, correct skid heights, and full AI/garage properties. Inspect camera occlusion from below and drive both passages. Nürburgring gets bridge metadata only if its actual layout requires it.

**Commits, validation and completion**

Land the performance prerequisite separately, then exactly one reviewed feature commit for each of the three stages above. Each feature commit includes its newly admitted modules, explanatory prose, property changes and reproducible report. A property correction must explain the wrong proxy, state the replacement physical claim and contain a negative example that still fails. No skipped candidate-specific failures or silently widened bars.

For each completed commit run:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo test --locked --lib the_circuits -- --ignored --nocapture
```

Run the ignored garage report after the proportions and bridge stages. Before pushing, fetch main, review the exact diff, incorporate upstream changes and re-run affected checks. Push each passing commit normally to `main`; never force-push or push an unfinished stage. The user's request authorizes those implementation pushes when complete; this planning pass does not claim they have happened.

Completion means 40 pinned source ids appear exactly once in the menu, every old and new circuit passes the same applicable contracts, screening reports no unaccounted failures, lookup performance has measured evidence, incompatible ghosts are rejected, and keyboard/gamepad plus bridge visual checks have been performed. Plan experiments alone do not satisfy that gate.

**Reproducing the research**

The CSVs in [docs/track-screening](docs/track-screening) contain all 40 rows at today's settings, at a 5 m target, and at that target with the three scale experiments. `old_profile_edge` is the old `min(7, 0.85 * radius, separation / 2)` envelope, not a fitted variable verge. Empty separation means the old algorithm compared no pair. `raw_crossings` counts proper raw-segment intersections, not bridge certification. Elevation is deliberately absent from these geometry probes.

```sh
git clone https://github.com/bacinger/f1-circuits.git /tmp/todora-circuits
git -C /tmp/todora-circuits checkout 394d8fbe70ef2c0b0c8d23ff7bee61fa09606055
python3 tools/screen_tracks.py /tmp/todora-circuits /tmp/todora-screen
python3 tools/screen_tracks.py /tmp/todora-circuits /tmp/todora-screen-5 --min-radius 5
python3 tools/screen_tracks.py /tmp/todora-circuits /tmp/todora-screen-scales \
  --min-radius 5 --scale az-2016=2 --scale mc-1929=1.25 --scale us-2022=1.25
```

Each run saves its Rust inputs, executable, CSV and provenance in the requested output directory. It builds the current Bevy dependency through Cargo, then compiles the actual ribbon source into the diagnostic executable. The radius override affects that scratch copy only. Revisit the utility when the geometry pipeline changes: its scales and envelope formula intentionally describe the baseline being investigated here.
