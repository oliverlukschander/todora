# Todora

A 3rd-person racer in [Rust](https://www.rust-lang.org/) and [Bevy](https://bevy.org/). Drive a comic shooting-brake around scaled real circuits — the Red Bull Ring, Spa-Francorchamps and Monza so far, with **T** between them.

The car is the ruler (~2.4 m long, ~1.1 m wide). Every circuit is shrunk the same way — the same 0.4⁄3 in plan, the same 0.28 in elevation, the same 8 m road — so they are comparable: Spielberg comes out a 528 m lap, Monza 855 m and Spa, which is half again as long as Spielberg in life, 868 m. Scaling each circuit to a target length instead would have made the long ones tighter and the short ones emptier, and the same lap time would have meant a different thing on each.

The hills come from a real elevation model and are kept as steep as it says, within a backstop. That backstop used to do the shaping rather than back it up: at 12% nearly half of Spielberg and well over half of Spa came out pinned to it, so every hill was the same ramp and only its length varied. Eau Rouge had neither a dip to drop into nor a climb to haul out of. At 20% the model shapes it again — Eau Rouge steepens through the compression the way it should, and a long descent now carries the car past its own top speed, because the engine stops pushing at the limiter and gravity does not.

The corners are exaggerated, and they have to be. The plan is shrunk about seven and a half times but the road only about one and a half, so the road comes out five times too wide for the land it is laid on. Left alone that turns a chicane into a straight: the whole of Monza's Rettifilo displaces this car by less than half a road width, so the quick way through it is not to steer at all. Each circuit is therefore pushed away from its own mean line — a low-pass of the centreline is where the circuit goes, and what is left over is the corners, so amplifying only the leftovers makes a corner a corner without moving the circuit. Long sweeps and straights come through untouched; chicanes come through multiplied. Monza turns twice as much per lap as it did, Spa a third more.

How far is the circuit's business, not a matter of taste, and it is set per circuit in `Circuit::corners`. Spielberg is wound tightly around a hillside and passes within 15 m of itself, so it has almost no room and is left as surveyed; Monza is three long straights and takes three and a half times. `corners_are_corners` is what says whether a circuit got enough, and `every_circuit_carries_a_road` is what says when it has had too much and has started growing into itself.

The circuit is one closed spline plus one cross-section profile. Tarmac, edge lines, kerbs and grass are strips of a single loft over the same stations, so they share their edges exactly — nothing overlaps anything except the corner markers below, and those do it on purpose. How wide that cross-section may be is set by the circuit itself, not by taste: the road never changes, and the verge either side narrows to whatever the circuit has room for. See `PROFILE` in `src/track/profile.rs`.

Corner markers run up the verge and round the bend: little diamonds either side, standing 0.15 m off the grass on a footprint 0.4 m point to point, one every 3.2 m, starting 16 m before the turn and carrying on through it to the exit. They belong to the circuit and to nothing else. A marker says where the corner is and nothing about how to drive it, so nothing about them knows what the car can do: the same curvature the road is lofted over says where the corners are, bends less than 6 m apart count as one so a chicane is one line rather than four, and each stretch of marked road is laid from its head, so the spacing is exact all the way down it and the last diamond of the approach lands on the corner entry itself.

They run through five colours, one to a diamond, and start again: yellow, blue, red, purple, orange — the first five pool balls, which is where the diamonds came from and which is a set picked to be told apart across a table at a glance. Five is also how many diamonds the approach is, so one turn of the cycle is exactly the braking zone: from the diamond where the brakes go on for the slowest corners you see the whole palette once, in order, and the yellow coming back round is the corner entry. A line of identical marks cannot say that — you would have to count them, and counting is what there is no time for. White is not in the palette on purpose, because white is what the edge lines and the kerb stripes are.

They stand up, and that is most of what makes them readable. From the driving seat the road ahead is seen almost edge-on: at 60 m the line of sight down to the verge is about three degrees, so a mark lying flat on it presents a twentieth of its own size and has all but gone by the time it matters, while a mark standing up presents nearly all of its height at any distance it can be seen at. Height is what a mark needs in order to keep its size, and it costs nothing on the ground — the footprint is the same 0.4 m diamond, because that was never the part that was too small. So a diamond is cut rather than drawn: the same four corners on the verge brought up to a point 0.15 m above the middle of them — about the height of one of the car's wheels, and three times the kerb's own lip, four faces and no bottom, standing straight up out of a verge that is falling away under it. Nothing collides with them; the car runs through one the way it used to run over one, because a marker that could be hit would be a marker you went round rather than one you read.

A pyramid is also the shape that gains height without gaining a vertical face, and that matters more than it sounds: a face standing on its edge would be a face seen edge-on from the car, a mark that disappears at the exact angle it is being read from, with nothing for the light to catch. `loft_faces_up` says every triangle in the circuit's mesh leans further up than sideways, and it still covers the markers rather than being told to look away from them — the steepest face of a diamond, on the low side where the verge falls, keeps forty-one degrees between itself and vertical.

These are the one mark on the circuit the loft cannot make. Every other one — the kerb stripes, the edge lines, the start/finish paint — is a strip of the sweep taking a colour, which fixes what shape it may be: a rectangle, square to the road, at least one station long, its whole band wide and flat on the ground. A diamond is a fifth of the width of the band it sits in, turned forty-five degrees to the road, and standing up off the ground, so no way of writing the table produces one. Each is twelve vertices of its own instead, four triangles flat-shaded off their own corners so the sides of a marker catch the light differently and it reads as a solid. They go into the loft's own mesh all the same, so there is still one surface, one material and one thing to replace when the circuit changes. Spielberg carries 214 of them, Monza 360 and Spa 416.

Carrying them through the corner is the point of them. The diamonds ahead are the shape of the bend before you can see the shape of the bend — where it tightens they crowd up on the inside, where it opens they run away from you — and the part of a corner you cannot see is the part after the entry. The approach reaches 16 m back because the car's hardest stop, top speed down to the tightest corner the game allows, takes 15.1 m, so the first diamond of a line is the brakes for the slowest corners and the ones after it are the brakes for everything quicker. They are all the same diamond the same distance apart because a dotted line is read as a rhythm, and one bigger than the next, or further from it, has to be identified before it can be read.

```sh
cargo run
```

**W** and **S** are the pedals — **S** brakes while you are rolling and reverses once you have stopped. **A** and **D** steer, **Space** is the handbrake, **R** starts again — car on the grid, clock at zero, marks wiped — **T** moves to the next circuit, **Scroll** zooms. Arrow keys work too, and **Shift** still brakes. **1**, **2** and **3** slide the setup between understeer, balanced and oversteer. **G** shows and hides the ghost. **Esc** stops the game where it stands and **Enter** starts it again. A gamepad works alongside: left stick steers, the triggers are the pedals, the south button is the handbrake, the D-pad slides the setup, the north button toggles the ghost, Select changes circuit and Start restarts. The stick and triggers are analogue, and on this car that is the difference between full lock and the lock you meant.

Every lap is a flying lap. The car is set down 45 m short of the start/finish line and the clock is armed by the line rather than by the car moving off, so the run up to it is yours to spend and costs nothing. 45 m is what there is to give: the car reaches 20 m/s in it, of the 22.2 m/s it actually settles at — `top_speed` is 24, but that is only where the engine's push fades out, and drag and rolling resistance are still there when it does — and the last tenth of that would cost another 65 m. The run-up also wants to be straight, or the car arrives slower for having cornered on the way, and straight is what the circuits have least of behind their lines: Spielberg has 48 m, Monza 97, and Spa has none at all, because Spa's line is inside La Source. So Spielberg and Monza cross the line at 20 m/s and Spa comes out of the hairpin at half that, which is still a rolling start and is still Spa.

A pause is the clock, not a second game running slowly behind it. Everything Todora does is driven by one virtual clock: the engine steps `Time<Fixed>`, which that clock fills, and the lap timer counts those same fixed steps. Stop the clock and both stop, together and exactly, because neither is given anything — no system has to be told it is paused and none of them can drift out of step with another. What a pause cannot stop is the driver's hands, so the controls are let go of on the way in and the keys are not read again until the way out: while the game is stopped, nothing the driver presses reaches the car, the reset key and the setup slider included.

A restart is for the lap, not for the day. It gives up the lap in progress and nothing else: the laps you have driven, the best of them and the ghost all stay, because you press **R** when the lap has gone wrong and what you are chasing is the lap you are not driving. Changing circuit clears all of it, because it belongs to the circuit you have left.

The handling is an arcade model, not a tyre simulation: a velocity, a heading, and a grip figure that says how far the two may disagree. Steering sets how fast the car rotates; grip is how much sideways speed the tyres can scrub off each frame. Ask a corner for more than that and the car keeps rotating while its velocity does not follow — the difference is the slide, and the tyre marks. Let go and the slide pulls the nose back into line. The handbrake and a heavy right foot at the limit both pull grip down, which is where the drifts come from; braking never does, so the brakes always work however hard you are turning. A slide costs speed — scrubbing rubber turns it into heat, and a rear that has let go spins its wheels rather than driving — so drifting is a technique, not a free lunch: hold the throttle through one and it bleeds, lift and it catches. Grip rises with speed, so a fast corner is a wide one rather than a lost one, and braking into a corner tightens it rather than loosening it — arriving too fast, the intuitive thing to do is the right one. Off the throttle the engine holds the car back hard — about half of what the brakes do — so the throttle is the speed control and the brakes are for the big stops: lift for a corner and the car settles into it. It also means a hill only gives speed while you are on the power. The grass is a gravel trap: at pace it costs most of a g and the wheels spin, so going off ends the corner, though a car can always crawl back to the road. The meter top left shows speed, what the seat feels, and where the setup slider sits.

Your fastest lap drives again alongside you as a ghost — a translucent copy of the car replaying it in step with the current lap's clock — and the panel top right shows the gap to it: green and negative when this lap is ahead at this point of the circuit, red and positive when behind. The gap is keyed to where you are, not to the clock, which is what makes it something you can drive against corner by corner.

The ghost outlives the session. Beat your own time and the lap is written out — a few thousand poses against the clock, one file per circuit, under `Library/Application Support/Todora/laps` on a Mac. Come back tomorrow and it is on the grid with you, its time already on the board as **BEST**. The file carries a fingerprint of the finished centreline — not of the trace it came from, so it moves when anything that shapes a circuit moves, the scales and the smoothing and the grade backstop included. A lap saved around one shape is quietly not a lap any more rather than a car driving through the scenery. Nothing about saving is allowed to be a problem: nowhere to write, a half-written file, a file somebody has edited — each one means no ghost, said once in the log, and a game that carries on.

There is no front and rear axle here to hand grip to, so understeer and oversteer are built out of what the model does have — the slide, the angle between where the car points and where it is going. The slider moves five dials together: how much more turn full lock asks for than the grip can give, how hard the nose is pulled back into line, how much grip the throttle spends at the limit, how hard a rear the driver has let go throws the tail round, and how eagerly the car rotates. That fourth one is what makes oversteer oversteer — on the loose setup, full throttle in a corner brings the tail round and the car turns more than the wheel asked; lift, and it catches. Every notch still turns at full lock and still refuses to spin; which one is quickest depends on who is driving. A driver who is still learning the circuit covers more ground on understeer, and one who is not is fractionally quicker on oversteer. The engine steps at a fixed 240 Hz whatever the frame rate, so a slow machine and a fast one drive the same car, and everything a car is lives in one `Handling` value, so a second car is data rather than code.

Needs a recent stable Rust (`rustup` on macOS). First Bevy compile is slow; later ones are not.

## Layout

| Path | What belongs there |
|---|---|
| `src/main.rs` | Binary entry. Calls `todora::run()`. |
| `src/lib.rs` | `GamePlugin` — register new feature plugins here. |
| `src/<feature>.rs` | One plugin per feature (`input`, `camera`, `world`, `lap`, `hud`, `skid`, `pause`). |
| `src/car/` | `physics.rs` is the engine: a pure `step` over a `Car`, its `Handling` (everything that makes one car drive like itself, as a value) and a `Surface`. `mod.rs` is the entity — the leaning body, the wheels, and `advance`, which carries `Controls` through the engine in fixed substeps. `driver.rs` is the AI: it laps the circuit in the tests today and drives opponents tomorrow. |
| `src/track/` | `circuits/` is one file per circuit, each a real trace in real metres, and `circuits/mod.rs` is the list the track key walks; `ribbon.rs` turns a trace into a centreline, opening the corners too tight to loft and exaggerating the ones too shallow to drive; `markers.rs` reads the corners back off it and stands the line of diamonds into and through them; `profile.rs` is the cross-section — the mesh, the height under a wheel and the grip under it all read from one table — and fits it to what a circuit can carry; `mod.rs` is the `Track` the game asks where the ground is, and the switch that builds the next one. |
| `src/ghost/` | `mod.rs` is the replay and the gap to it; `store.rs` is the lap on disk — where it lives, and what a file has to be before it is believed. |
| `assets/` | Runtime files Bevy loads. glTF lives in `assets/models/`. |
| `art/` | Source art. Blender files in `art/models/`. |
| `tools/` | Generators and one-off scripts. `make_track.py` turns a real circuit into a `src/track/circuits/` module. |

New gameplay goes in its own `src` module with a plugin, then gets added to `GamePlugin`. New runtime files go in `assets/`. Source meshes stay in `art/`.

Input runs in `PreUpdate`, after Bevy reads the devices and after the pause has decided whether the driver is being listened to at all; the track key runs next, and everything that puts itself back on a reset runs after that — so the circuit is already the new one by the time the car, the clock, the ghost and the marks act on it. Driving runs in `FixedUpdate` at 240 Hz; each step includes track confinement and recovery. The lap clock follows the car on that same clock, then the ghost finishes and records the step. Camera, body animation, ghost replay and HUD run in `Update`, before Bevy propagates transforms for rendering. Keep that order when adding systems. Bevy carries fractional steps between frames and caps a stall at its default 250 ms of virtual time; driving and lap timing share that cap.

## Another circuit

```sh
tools/make_track.py be-1925 spa_francorchamps "Spa-Francorchamps"
```

That is the whole job. The id is a circuit in [bacinger/f1-circuits](https://github.com/bacinger/f1-circuits), which traces centrelines as GeoJSON; the tool projects the trace flat about its own centroid, samples the [Open-Meteo](https://open-meteo.com/) DEM for heights, writes `src/track/circuits/<name>.rs` and adds it to the list. Nothing is scaled there — `src/track/mod.rs` does that, the same way for every circuit.

A circuit then has to earn its place, and the tests are what decide. Corners tighter than the loft can carry are opened out before the road is swept, which cuts them; the circuits in the game keep between four fifths and all of it that way; a circuit that keeps far less has been rounded off into a ring rather than shrunk — Monaco comes out of 3.3 km as a hundred-metre loop, and `every_circuit_survives_the_shrink` is what says so. What a circuit can carry either side of the road it settles for itself: the 8 m road never moves, and the verge narrows to fit the tightest corner and the closest the circuit comes to itself. So adding one is additive — a circuit with less room gets a narrower verge instead of everyone else getting one.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

Tests cover acceleration, braking, reverse, slopes, grass, drift recovery, all three setups, wall impacts, lap validation and ghost recording. Every circuit in the game is held to the same bar — it survives the shrink, carries a road, laps as one lap, closes its kerb stripes at the line, starts on the tarmac, marks its corners with a line of diamonds all the same shape and all the same distance apart, running through its palette once over every braking zone, laid round the bend as well as up to it, standing on the verge with no face of them near vertical and never anywhere there is no bend, keeps most of the relief its elevation model gave it, has corners a straight line does not fit down, and is lapped by both AI drivers within a time budget set by its own length — so a new one either clears it or fails loudly. The relief is checked as a fraction rather than in metres, because Spa rises 27 m and Monza 6, and Monza is not broken. A restart keeps the board and the ghost while a switch clears both, the track key builds the next circuit before anything puts itself back, and the saved lap is checked both ways round and against every way a file can be wrong. The schedule tests compare equal driving time across different frame rates and stalls, and check that resets and setup changes reach the next physics step. Mixed-input stress tests check finite values and bounded speeds and forces. AI drivers exercise every circuit through the same movement path as the game, which is the net under the hills: a climb the car cannot take or a descent it cannot stop on fails there rather than under the player.

Run the optional handling reports with:

```sh
cargo test --locked --lib -- --ignored --nocapture
```

Automated checks do not replace a drive with a keyboard and gamepad when judging steering feel, camera motion and analogue controls.

## A Mac app

```sh
tools/package_macos.sh
```

Builds a release binary, wraps it in `dist/Todora.app` with its icon, and puts that in `dist/Todora.dmg` next to an Applications shortcut. It uses only what macOS ships with: QuickLook renders the icon from `art/icon/todora.svg`, `iconutil` packs it, `hdiutil` makes the disk image. The app is signed ad hoc, which is all this Mac needs; another Mac will ask for right-click → Open the first time, because there is no Developer ID behind it.

Rebuild the shooting-brake from Blender:

```sh
/Applications/Blender.app/Contents/MacOS/Blender --background --python tools/make_shooting_brake.py
```
