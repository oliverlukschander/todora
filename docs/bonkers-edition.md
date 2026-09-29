# Bonkers Edition

This branch (`play/bonkers-edition`) is Todora with the seriousness taken out.
The circuits are still the circuits and the handling is still the handling. What
is different is what you ride, how high the road goes, how fast, what it looks
and sounds like, and what happens to you on the way round.

It lives in one new module, [`src/fun`](../src/fun), that stands *around* the
engine and never inside it. See [Integrity](#integrity) for why that matters.

## Three levels

**Settings → Fun → Silliness**, or **F9** while driving or paused to step round
them (Serious, Silly, Bonkers, and back to Serious).

| | Serious | Silly | Bonkers (default here) |
|---|:-:|:-:|:-:|
| The game as shipped | yes | | |
| A chicken to ride (or a duck, a bathtub, a trolley), a helmeted rider, hats, googly eyes | | yes | yes |
| Tyre smoke, feathers, confetti, fireworks, sparks, exhaust flames | | yes | yes |
| Camera that widens with speed, shakes on impacts, leans into corners | | yes | yes |
| A horn (**H**) that is a roulette; an announcer and a fun score; jokes on the title, pause and loading screens | | yes | yes |
| Tube-men waving beside the road | | yes | yes |
| Neon rails, bloom, colour grading, lasers, underglow | | | yes |
| Generative techno that builds with your speed | | | yes |
| Faster (up to 3.6×), hillier circuits, boost pads, jump pads, mystery boxes, cows, cones, bowling pins, balloons, chaos events, a hop (**E**) | | | yes |
| Laps count | yes | yes | **no** |

Silly changes how the game *looks and sounds* and nothing about how it
*drives*, so a lap in Silly is a lap like any other and counts. Bonkers changes
the drive, so a Bonkers lap is flagged the moment it starts and never becomes a
record, a ghost, a medal or a leaderboard entry. Its card says **BONKERS!**
where an invalid lap says INVALID, in the purple the clock uses for it rather than
the red of a mistake.

Bonkers is the default on this branch, so a settings file from `main` starts in it
and no lap counts until F9 or the settings page says otherwise. To make Serious the
default instead, move `#[default]` to it on `Silliness` in
[`src/fun/mod.rs`](../src/fun/mod.rs).

Under Silliness the Fun tab has a row for each crazy thing, so Bonkers with the
stock car, or Bonkers with normal speed, or Bonkers with flat circuits, is a
setting and not a code change:

- **Ride:** Car, Chicken, Rubber duck, Bathtub, Shopping cart
- **Hat:** Surprise me (a different one each start), none, party, top, crown,
  traffic cone, propeller, chef's, cowboy
- **Googly eyes:** on or off
- **Speed:** Normal, Fast (1.6×), Ludicrous (2.4×), Plaid (3.6×)
- **Track wildness:** As surveyed, Rolling hills, Rollercoaster, Absurd
- **Neon**, **Techno**, **Chaos events:** each on or off

### With company

Split-screen and shared practice both need everybody driving the same car on
the same road, so the fun layer stands down for them: from the split-screen
lobby to the end of the race, and for as long as a shared practice is under way,
the game is Serious whatever the settings say, and F9 does nothing. When it is
over the settings come back. The circuits are built as surveyed for the length
of it and as wild as asked afterwards, which costs a loading screen each way if
hills were on.

## What is in it

**The chicken.** A rooster in a saddle in the car's paint, built out of
primitives (no model file) with a helmeted, scarfed rider. Its legs run at the
speed you do, its head stays put while the rest of it goes past, its wings come
out when it is airborne or boosting, its comb and hat sway on springs, and its
googly pupils slosh about under every corner. Its engine is clucking: faster
the faster you go.

**Wild tracks.** (A change of wildness on the settings page is built when the page
is closed, once, and not for every step of its row.) The layout of a circuit is never
touched, so it is still
recognisable on the mini-map; only the elevation changes: more of the surveyed
relief, hills on top, and ramps on the straights, all of it fading out to
nothing at the start line and at bridges. The car leaves the road exactly where
a thrown stone would, when the road falls away faster than gravity pulls, so
*the hills decide where the jumps are*. The steepest a wild road is allowed to
be is 28%, 40% or 60% for the three settings, where the shipped roads are held
to 20%. Every one of the 51 circuits builds at every setting.

**Speed.** Not "turn everything up". Asking for `k` times the speed scales
every speed by `k`, every acceleration by `k²` and every rate by `k`, which is
the same car taking the same corners in `1/k` of the time. The plain AI driver
laps all 51 circuits at 1.6×, 2.4× and 3.6×, on every wildness, faster than at
the last speed but by less than `k` (the corners set the pace), and at the
shipped speed on the shipped roads. What it cannot do is always drive the
*shipped* speed on the hilly ones: it cannot see a crest coming, and three of
the 153 circuit-and-wildness pairs put a wheel off the road or stall on the
steepest climbs. That is the shipped car on a hill it was never built for, and
a person can do better; it is also only ever a Bonkers lap, which never counts.

**Lights and techno.** Bloom, a colour grade that turns the whole world through
the rainbow and flashes on the beat, chromatic aberration, a vignette that closes
in with speed, a barrel distortion that boosts stretch; neon rails on both kerbs
and a dashed line whose rainbow scrolls down the road, chasing pylons, laser
beams, a pool of light under the car, streaks of light in the air at speed. The
music is a step sequencer on the audio thread — four-on-the-floor kick with a
sidechain duck, offbeat acid bass, claps, hats, a delayed arpeggio, pads, a
riser and a fill every eight bars — whose layers fade in with your speed and
boost. The lights are locked to its beat.

**Things on the road.** Boost pads (chevrons that stream the way the lap
runs), jump pads (a ring across the road), mystery boxes in threes, and set
pieces laid out per circuit: a cone slalom, ten bowling pins in a triangle (they
knock each other over, so a strike is possible), cows crossing, a parade of
ducks, watermelons, stacked boxes, a balloon arch. Each has its own noise, its
own debris and its own points. Where they go is a plain function of the circuit,
so a circuit always has the same things in the same places.

**Things that happen to you.** Every half minute, and whenever a box is opened,
one of: moon gravity, an ice rink, turbo time, swapped steering, giant mode,
tiny mode, hyperdrive, disco fever, fisheye, a camera barrel roll, cows falling
out of the sky. Only what would do something is picked: no giant car when there
is no mount to swell, no disco with the neon off, no barrel roll for anyone who
asked for reduced motion.

**Score.** Fun points for airtime, drifts and smashing things; each within a
couple of seconds of another is worth more, up to ×8. There are ranks. There are
fifteen extra awards, counted over every session and listed with the others when
the game is Silly. The seventy-one that shipped are unchanged.

## Controls added

| | Keyboard | Pad |
|---|---|---|
| Horn | **H** (hold to repeat) | none to begin with |
| Hop / glide (chicken) | **E** (hold in the air to glide) | none to begin with |
| Step the silliness round | **F9** | |

The first two are rebindable like the rest, and a pad button for either is
chosen in Settings → Keys (every button already means something, and the right
stick's click starts a shared practice in the builds that have one, so neither
takes one by default). A settings file from before them that already has H or E
bound to something else keeps that, and the newcomer goes without. Everything
else drives as before.

F9 shows what it landed on at the top left, under the name of the circuit.

## Integrity

The official leaderboard verifies runs by replaying the recorded inputs under
the stock physics. So nothing in this module edits [`car::advance`](../src/car/mod.rs),
[`physics::step`](../src/car/physics.rs) or how the track holds the car, and the
determinism digests in `src/car/determinism.txt` pass unchanged. What the fun layer
does instead:

- **Tweaks** change the [`Handling`](../src/car/physics.rs) *on its way into* the
  engine: scaled for speed, boosted, stripped of grip in the air. When nothing is
  asked for, the value passes through untouched, bit for bit
  (`an_unasked_for_tweak_changes_nothing`).
- **Air** is a height carried *beside* the car. The engine still glues the car to
  the road; the model and the camera ride on top.
- **Wild tracks** are only built for driving: the game asks for them through the
  `WildRequest` resource and `Track::with_wild`. `Track::new`, which is what the
  tests, the verifier and saved ghosts use, builds the circuit as surveyed, and a
  wild circuit has a different surface fingerprint
  (`a_wild_road_is_the_same_road_only_higher`), so its laps can never be
  mistaken for a stock one.
- **Bonkers laps are flagged** in the lap clock the same way an assisted lap is
  flagged as not going online, so nothing downstream needed to learn about them.
  It is the first reason the clock keeps, so the card says BONKERS! and not
  whatever else went wrong later on the lap. A lap that is running is flagged
  before each step of it is judged, so the level changing on the very step that
  finishes a lap does not get that lap through, and the lap a step starts is
  flagged as it starts (`a_lap_that_bonkers_had_a_hand_in_never_counts`).
- **Leaving Bonkers puts the car back on the grid.** The car is still going at the
  speed Bonkers gave it, and the next lap that started on that would count, and
  be accepted by the server if it came in under a quarter over the top speed.
  F9 and the settings page both do it, and so does entering split-screen.
- **What the awards count** leaves Bonkers out: the kilometres are the kilometres
  of the game as shipped, and a Bonkers lap is not a lap that missed, for the
  once-only hint that suggests Beginner mode.
- **Company switches it all off** (see [With company](#with-company)), so two
  people in one race are never on different roads.

## Reduced motion

**Settings → HUD → Reduced motion** stops the flashing and most of the movement: the
colour grade holds one hue, the aberration and lens effects are off, the lasers
stand still, the rails keep their glow but not their pulse or their scroll, the
underglow and the mystery boxes hold one colour, a boost does not brighten the
world, nothing shakes, banners appear and fade instead of springing and floating,
there are no fireworks or backfire sparks or star bursts, and the camera never
barrel-rolls; the field of view widens a third as much.

With it off, everything that follows the beat follows the beat of the music, which
is 140 bpm (2.3 a second), and the clock that keeps it never runs faster than a
quarter over that to catch up with the audio: a gap of more than a beat is stepped
over, once. The colour grade turns at a rate it is *told*, added up a frame at a
time, and does not lurch when a disco starts or stops
(`the_hue_turns_steadily_through_a_disco_however_long_it_has_been_going`). There is
no full-screen white flash.

## Looking at it

Sounds and music are synthesised, so they can be rendered and inspected:

```sh
cargo test --lib write_the_sfx -- --ignored          # dist/sfx/<name>.wav, every sound
cargo test --lib write_the_techno -- --ignored       # dist/techno.wav
ffmpeg -i dist/sfx/Cluck.wav -lavfi showspectrumpic=s=800x300:scale=log:fscale=log spec.png
```

The visual-check harness (`--features visual-check`, see the top of
`src/visual_check.rs`) has gained an orbit camera, an AI autodrive, burst
capture and a frame timer, so that things that move can be looked at moving:

```sh
BEVY_ASSET_ROOT=$PWD TODORA_CAPTURE=/tmp/shot.png TODORA_CIRCUIT=monza \
  TODORA_DRIVE=1 TODORA_AT=88 TODORA_FRAME=50 TODORA_BURST=12:20 \
  ./target/debug/todora
TODORA_CAM=170:12:5.2 …        # the chicken from the front
TODORA_TIME=600 TODORA_DRIVE=1 …   # how long 600 frames take, instead of a picture
```

Where things are on a circuit, for finding something to look at:

```sh
TODORA_SHOW=monza TODORA_WILD=2 cargo test --lib show_the_course -- --ignored --nocapture
```

Every circuit at every speed and wildness, as a table of AI lap times, every
circuit's wild roads, and (slowly) the road, ground and scenery of every circuit
at every wildness, as loading one from the menu builds them:

```sh
cargo test --lib every_circuit_can_be_lapped -- --nocapture
cargo test --lib every_circuit_takes_every_wildness -- --nocapture
cargo test --lib every_circuit_loads_at_every_wildness -- --ignored
```

## Where to look in the code

| | |
|---|---|
| `src/fun/mod.rs` | the levels, `Fun` (what is in force this frame) and `Together` (the game has company) |
| `src/fun/mount/` | the chicken, duck, tub and trolley, the rider, the hats, the pose |
| `src/fun/tweak.rs` | speed, boost, ice, air: changes made to the handling on its way in |
| `src/fun/air.rs` | leaving the ground: crest launches, hops, gliding, landing |
| `src/fun/course.rs` | what is on the road and where: a plain function of the track |
| `src/fun/pads.rs`, `props.rs`, `events.rs` | pads and boxes; cows, pins and balloons; things that happen to you |
| `src/fun/announcer.rs`, `awards.rs`, `quips.rs` | banners and the score; the extra awards; the jokes |
| `src/fun/disco.rs`, `beat.rs`, `juice.rs` | the lights; the beat clock; the camera |
| `src/fun/horn.rs`, `hotkey.rs` | the horn roulette; F9 |
| `src/fun/particles.rs`, `parts.rs`, `rng.rs` | things that fly; the shared shapes and paints; a small RNG |
| `src/track/ribbon.rs` (`make_wild`) | the hills |
| `src/sound/sfx.rs`, `techno.rs` | the synthesised noises, and the music |
| `src/text/fun.rs` | every word of it, in five languages |

## What was not verified

- **Nothing was listened to.** The noises and the music were checked the way
  a sound can be checked without ears: rendered and looked at as spectrograms
  and waveforms (the clucks fall, the moo's vowel formants move, the kick lands on
  every beat, the techno's layers stack up and its phrases end in a riser and a
  gap), and covered by tests for pitch, level, silence at the end and no clicks.
  Whether the chicken sounds like a chicken and the kick sounds good is for a
  person with speakers to say, and the recipes in `src/sound/sfx.rs` are data.
- Run on Linux (Asahi, aarch64) only. Nothing was tried on macOS or with a real
  game pad. Standing down for split-screen and shared practice is covered by a test
  of the one system that decides it, and has not been watched in a real
  split-screen race or a real Game Center session.
- **Speed is one measurement.** On that machine Bonkers on Monza with the AI
  driving holds 60 fps, the display's rate, with the game's own systems taking
  about a third of a millisecond more of each frame than Serious does. What a
  slower graphics card or a bigger window does to bloom and the rest is not known.
- The German, French, Spanish and Italian text for the fun layer has not been read
  by a native speaker, like the rest; the jokes were adapted rather than translated
  word for word.
- The camera keeps the car in sight on every circuit at every setting up to the
  last quarter of the way to it (and on a road with hills it looks again, closer,
  wherever the ground changes suddenly; on the circuits as surveyed it is the
  camera that shipped). On the wildest road a crest a metre behind a car that has
  just come over it can still hide the car for a moment.
- The layout of the settings page changed to fit: wider, with the rows and the tabs
  closer together, so that seven tabs fit in the longest language and the Keys tab
  fits the screen at 110%. That is measured on paper, from the fonts, and not
  looked at in every language.
- The rubber duck, bathtub and trolley are simpler than the chicken and have had
  less looking at.
