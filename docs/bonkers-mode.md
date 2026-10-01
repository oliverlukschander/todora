# Bonkers mode

Todora has two ways to play. **Drive**, on the title, is the game as it shipped.
**Bonkers mode**, right under it, is the other: you ride a chicken (or a rubber
duck, a bathtub, a shopping trolley) round circuits that have grown hills, over
boost pads and jump pads, through cows and bowling pins, under neon, to techno,
and things happen to you on the way round. Its laps never count.

It lives in one module, [`src/fun`](../src/fun), that stands *around* the engine
and never inside it. See [Integrity](#integrity) for why that matters.

## Playing it

| | |
|---|---|
| Play it | **Bonkers mode** on the title, under **Drive** |
| Switch, while driving or paused | **F9**, or **Settings → Bonkers → Bonkers mode** |
| Change what it is like | **Settings → Bonkers**, the rows under the switch |
| Horn, hop | **H**, **E** (a pad button for each can be chosen in **Settings → Keys**) |

The game always opens on the plain mode. Bonkers mode is never saved, so nobody
finds themselves in it by surprise, and the week's challenge on the title is
always driven in the plain mode, because its laps are meant to count. What
Bonkers is *like* is saved. While **Bonkers mode** is the button chosen on the
title, the logo goes through the rainbow, a sticker says so and the tagline is
a joke, which is what the mode is like before it is driven.

In the plain mode there is nothing of it: no chicken, no hills, no jokes, the
camera, the HUD and the sound as they shipped. On the Bonkers tab every row
under the switch is dimmed until the switch is on.

The rows:

- **Ride:** Car, Chicken (the default), Rubber duck, Bathtub, Shopping cart
- **Hat:** Surprise me (a different one each start), none, party, top, crown,
  traffic cone, propeller, chef's, cowboy
- **Googly eyes:** on or off
- **Speed:** Normal (the shipped car's, the default), Fast (1.15×),
  Ludicrous (1.3×), Plaid (1.5×)
- **Track wildness:** As surveyed, Rolling hills, Rollercoaster (the default),
  Absurd
- **Neon**, **Techno**, **Chaos events:** each on or off

F9 says which mode it has switched to at the top left, under the name of the
circuit: **BONKERS MODE!** in purple, the colour a Bonkers lap is shown in, or
**Normal mode**. Switching while hills are on builds the circuit again, which
costs a loading screen.

### With company

Split-screen and shared practice both need everybody driving the same car on
the same road, so Bonkers stands down for them: from the split-screen lobby to
the end of the race, and for as long as a shared practice is under way, the game
is the plain one whatever the settings say, and F9 does nothing. When it is over
the mode comes back. The circuits are built as surveyed for the length of it and
as wild as asked afterwards, which costs a loading screen each way if hills were
on.

## Steerable

The first Bonkers looked wonderful and could not be steered. It went 2.4 times
as fast as the shipped car by default, and 3.6 at the most, and it got there by
scaling everything: every speed by `k`, every grip and acceleration by `k²`,
every rate by `k`, including how fast the wheels turn and the car rotates. That
is the same car through the same corners in `1/k` of the time, which is right,
and nobody's hands are 2.4 times quicker, which is the trouble.

So what Bonkers offers is measured now, not guessed at.
[`src/fun/playable.rs`](../src/fun/playable.rs) drives it the way it is played,
over the hills and off them, across the pads, through the same engine and the
same air and tweaks the game uses, with the game's own two drivers: the plain
one, which steers perfectly and looks 48 m up the road, and the clumsy one,
which is a person on a keyboard (full lock or none, 150 ms behind, 18 m of
look-ahead, late on the brakes). The share of the drive the clumsy driver spends
off the road, over all 51 circuits:

| | as shipped | Bonkers 1× | 1.15× | 1.3× | 1.5× | 2.4×, the first Bonkers' |
|---|--:|--:|--:|--:|--:|--:|
| flat (as surveyed) | 35.3% | 36.8% | 39.8% | 43.9% | 48.3% | 65.5% |
| Rollercoaster | | 36.6% | 40.0% | 44.6% | 48.4% | 65.5% |

It is the speed that a person cannot keep up with, and at the shipped speed
Bonkers is as steerable as the shipped game, whatever the hills. (The plain
driver, which reacts at once and sees far, laps 2.4 times the speed as cleanly as
the shipped one: it was never the car.) The faster settings are there for anyone
who asks for them, and the fastest is still nearer the shipped game than the
first Bonkers was. What changed:

- **Speed.** Bonkers drives at the shipped car's speed unless asked for more,
  and more is 1.15, 1.3 or 1.5 times, not 1.6, 2.4 or 3.6.
- **Crests.** A car leaves the road where it falls away faster than gravity
  pulls; working that out from the road under the car meant smoothing it, and
  it was smoothed over a number of physics steps. At the old speeds that was a
  metre or more of road. At the shipped speed it is a few centimetres, the
  centreline is straight between stations 0.4 m apart, and every bend between
  two of them read as a crest: the car left the ground 24 times a lap on
  circuits with no hills at all. The road is read over a distance now, and a car
  that has landed settles over a distance too.
- **The air.** A car in the air keeps a third of its grip, and its steering, the
  way every arcade racer has it, so that a hop on the way into a bend comes down
  on the road. The pedals still do nothing until it is down. A crest on a bend,
  or just before one, that a car could not follow from the air at the speed it
  is going is driven over and not left: Monaco's crests come just before its
  corners, and the car used to fly into them with no brakes.
- **The hills keep to the straights.** The hills Bonkers adds fade out in and
  around bends, which keep the relief the circuit has of its own: a crest in a
  corner sends a car off the road on the outside, and a steep climb beside one
  strands whoever ran wide on the grass. On a straight a crest is a jump.
- **Jump pads** are only where the road runs straight to well past where the car
  comes down, and they throw it less high: two seconds in the air at the shipped
  speed, not 2.7.
- **Chaos** that takes the steering away is gentler, or gone: see
  [Things that happen to you](#things-that-happen-to-you).

The plain driver, which keeps all of its laps clean in the shipped game, kept 6%
of them clean on the first Bonkers' Rollercoaster hills at the shipped speed, and
keeps 86% of them clean now; the clumsy driver is fetched back from the grass two
and a half times as often as in the shipped game, where it was seven times as
often. `bonkers_as_it_comes_is_as_steerable_as_the_game_as_shipped` and
`even_the_fastest_bonkers_is_nearer_the_game_than_the_first_one` hold it to
that, and the whole table is

```sh
cargo test --lib bonkers_as_a_table -- --ignored --nocapture
TODORA_SPEEDS=1,1.5 TODORA_WILD=0,2 TODORA_PARTS=crests,jumps TODORA_CHAOS=ice cargo test --lib bonkers_as_a_table -- --ignored --nocapture
```

## What is in it

**The chicken.** A rooster in a saddle in the car's paint, built out of
primitives (no model file) with a helmeted, scarfed rider. Its legs run at the
speed you do, its head stays put while the rest of it goes past, its wings come
out when it is airborne or boosting, its comb and hat sway on springs, and its
googly pupils slosh about under every corner. Its engine is clucking: faster
the faster you go.

**Wild tracks.** The layout of a circuit is never touched, so it is still
recognisable on the mini-map; only the elevation changes: more of the surveyed
relief, and hills and ramps on top of it that keep to the straights, all of it
fading out to nothing at the start line and at bridges. The car leaves the road
where a thrown stone would, when the road falls away faster than gravity pulls,
so *the hills decide where the jumps are*. The steepest a wild road is allowed
to be is 28%, 40% or 60% for the three settings, where the shipped roads are
held to 20%. Every one of the 51 circuits builds at every setting. A change of
wildness on the settings page is built when the page is closed, once, and not
for every step of its row.

**Speed.** Not "turn everything up". Asking for `k` times the speed scales
every speed by `k`, every acceleration by `k²` and every rate by `k`, which is
the same car taking the same corners in `1/k` of the time. The steps are small,
for the reason in [Steerable](#steerable).

**Lights and techno.** Bloom, a colour grade that turns the whole world through
the rainbow and flashes on the beat, chromatic aberration, a vignette that closes
in with speed, a barrel distortion that boosts stretch; neon rails on both kerbs
and a dashed line whose rainbow scrolls down the road, chasing pylons, laser
beams, a pool of light under the car, streaks of light in the air at speed. The
music is a step sequencer on the audio thread (four-on-the-floor kick with a
sidechain duck, offbeat acid bass, claps, hats, a delayed arpeggio, pads, a
riser and a fill every eight bars) whose layers fade in with your speed and
boost. The lights are locked to its beat.

**Things on the road.** Boost pads (chevrons that stream the way the lap
runs), jump pads (a ring across the road), mystery boxes in threes, and set
pieces laid out per circuit: a cone slalom, ten bowling pins in a triangle (they
knock each other over, so a strike is possible), cows crossing, a parade of
ducks, watermelons, stacked boxes, a balloon arch. Each has its own noise, its
own debris and its own points. Where they go is a plain function of the circuit,
so a circuit always has the same things in the same places.

### Things that happen to you

Every half minute or so, and whenever a box is opened, one of: moon gravity, an
ice rink, turbo time, giant mode, tiny mode, hyperdrive, disco fever, fisheye, a
camera barrel roll, cows falling out of the sky. Only what would do something is
picked: no giant car when there is no mount to swell, no disco with the neon
off, no barrel roll for anyone who asked for reduced motion.

The ones that act on the drive were measured like the rest, each for a whole lap
at full strength (they last a few seconds), on Rollercoaster hills at the shipped
speed, and the ones that took the steering away were made gentler:

| | the plain driver's clean laps | off the road | a person off the road |
|---|--:|--:|--:|
| nothing happening | 86% | 0.9% | 36.6% |
| moon gravity, the first | 24% | 11.4% | 37.8% |
| moon gravity, now | 65% | 3.4% | 36.5% |
| hyperdrive, the first | 58% | 2.8% | 44.7% |
| hyperdrive, now | 86% | 0.7% | 41.4% |
| turbo time, the first | 38% | 5.4% | 58.3% |
| turbo time, now | 52% | 3.7% | 52.2% |
| ice rink, now | 72% | 1.5% | 34.8% |
| swapped steering | 0% | 57.8% | 87.6% |

(The first ones were measured one change to the air earlier, before a crest just
short of a bend stopped being left, when nothing happening was 80%, 1.3% and
36.8%.)

- **Moon gravity** is half the world's, not a quarter, and only for a car that
  is already in the air: whether a crest is left is judged against the world's
  gravity, so every jump goes higher and lasts longer and there are no more of
  them. At a quarter the car left the road at every bump.
- **Hyperdrive** adds a fifth to the speed, not a third, and the jumps are made
  at the speed in force. They were made at the speed the setting says, which
  under hyperdrive is a jump 70% longer than it should be, and a great many more
  of them.
- **Turbo time** pushes at 40% of a boost pad's strength, for three and a half
  seconds and not five. At full strength it delivered the car to every corner
  half as fast again as it could take it; it is still the hardest of them, which
  is what makes it a rush. A boost pad is as strong as it was: it is over in
  under two seconds, and measured, it costs nothing.
- **The ice rink** leaves 55% of the grip, not 30%, for six seconds and not
  seven. The drivers here are told the grip and slow down for it, so the table
  cannot show what it does to a person, who is told by a banner: at 30% a
  corner can be taken at little more than half its speed, at 55% at three
  quarters of it.
- **Swapped steering** is gone. It took the steering away, which is the one
  thing nothing in Bonkers is to do.

**Score.** Fun points for airtime, drifts and smashing things; each within a
couple of seconds of another is worth more, up to ×8. There are ranks. There are
fifteen extra awards, counted over every session and listed with the others in
Bonkers mode, and from then on once one of them has been earned. The
seventy-one that shipped are unchanged.

## Integrity

The official leaderboard verifies runs by replaying the recorded inputs under
the stock physics. So nothing in this module edits [`car::advance`](../src/car/mod.rs),
[`physics::step`](../src/car/physics.rs) or how the track holds the car, and the
determinism digests in `src/car/determinism.txt` pass unchanged. What Bonkers
does instead:

- **Tweaks** change the [`Handling`](../src/car/physics.rs) *on its way into* the
  engine: scaled for speed, boosted, given less grip on ice and in the air. When
  nothing is asked for, the value passes through untouched, bit for bit
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
  It is the first reason the clock keeps, so the card says **BONKERS!** and not
  whatever else went wrong later on the lap, in the purple the clock uses for it
  rather than the red of a mistake. A lap that is running is flagged before each
  step of it is judged, so switching on the very step that finishes a lap does
  not get that lap through, and the lap a step starts is flagged as it starts
  (`a_lap_that_bonkers_had_a_hand_in_never_counts`).
- **Leaving Bonkers puts the car back on the grid.** The car is still going at the
  speed Bonkers gave it, and the next lap that started on that would count. F9,
  the settings page and entering split-screen all do it.
- **What the awards count** leaves Bonkers out: the kilometres are the kilometres
  of the game as shipped, and a Bonkers lap is not a lap that missed, for the
  once-only hint that suggests Beginner mode.
- **Company switches it off** (see [With company](#with-company)), so two people
  in one race are never on different roads.

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
`src/visual_check.rs`) has an orbit camera, an AI autodrive, burst capture and a
frame timer, so that things that move can be looked at moving. `TODORA_BONKERS=1`
starts it in Bonkers mode:

```sh
BEVY_ASSET_ROOT=$PWD TODORA_CAPTURE=/tmp/shot.png TODORA_CIRCUIT=monza TODORA_BONKERS=1 \
  TODORA_DRIVE=1 TODORA_AT=88 TODORA_FRAME=50 TODORA_BURST=12:20 \
  ./target/debug/todora
TODORA_CAM=170:12:5.2 …           # the chicken from the front
TODORA_TIME=600 TODORA_DRIVE=1 …  # how long 600 frames take, instead of a picture
TODORA_SCREEN=title TODORA_ROW=1  # the title, with Bonkers mode chosen
TODORA_SCREEN=settings TODORA_TAB=6  # the Bonkers tab
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
| `src/fun/mod.rs` | the mode, `Fun` (what is in force this frame) and `Together` (the game has company) |
| `src/fun/playable.rs` | Bonkers driven by the game's own drivers: what the speeds and the rest were chosen from |
| `src/fun/mount/` | the chicken, duck, tub and trolley, the rider, the hats, the pose |
| `src/fun/tweak.rs` | speed, boost, ice, air: changes made to the handling on its way in |
| `src/fun/air.rs` | leaving the ground: crest launches, hops, gliding, landing |
| `src/fun/course.rs` | what is on the road and where: a plain function of the track |
| `src/fun/pads.rs`, `props.rs`, `events.rs` | pads and boxes; cows, pins and balloons; things that happen to you |
| `src/fun/announcer.rs`, `awards.rs`, `quips.rs` | banners and the score; the extra awards; the jokes |
| `src/fun/disco.rs`, `beat.rs`, `juice.rs` | the lights; the beat clock; the camera |
| `src/fun/horn.rs`, `hotkey.rs` | the horn roulette; F9 |
| `src/fun/particles.rs`, `parts.rs`, `rng.rs` | things that fly; the shared shapes and paints; a small RNG |
| `src/track/ribbon.rs` (`make_wild`, `straightness`) | the hills, and the bends they keep out of |
| `src/sound/sfx.rs`, `techno.rs` | the synthesised noises, and the music |
| `src/text/fun.rs` | every word of it, in five languages |

## What was not verified

- **Nobody has steered it since it was slowed down.** That it can be steered is
  what the game's own drivers say, the clumsy one in particular, which is a model
  of a person on a keyboard and not a person. The speeds were chosen from what
  they did, and from the user who drove the first one finding 2.4 times the speed
  impossible; whether 1.15 or 1.5 is still fun, or the shipped speed is fast
  enough to feel bonkers, is for someone with a keyboard to say.
- **Nothing was listened to.** The noises and the music were checked the way
  a sound can be checked without ears: rendered and looked at as spectrograms
  and waveforms, and covered by tests for pitch, level, silence at the end and no
  clicks. Whether the chicken sounds like a chicken and the kick sounds good is
  for a person with speakers to say, and the recipes in `src/sound/sfx.rs` are
  data.
- Run on Linux (Asahi, aarch64) only. Nothing was tried on macOS or with a real
  game pad. Standing down for split-screen and shared practice is covered by a test
  of the one system that decides it, and has not been watched in a real
  split-screen race or a real Game Center session.
- The German, French, Spanish and Italian text has not been read by a native
  speaker, like the rest; the jokes were adapted rather than translated word for
  word.
- The camera keeps the car in sight on every circuit at every setting up to the
  last quarter of the way to it (and on a road with hills it looks again, closer,
  wherever the ground changes suddenly; on the circuits as surveyed it is the
  camera that shipped). On the wildest road a crest a metre behind a car that has
  just come over it can still hide the car for a moment.
- The rubber duck, bathtub and trolley are simpler than the chicken and have had
  less looking at.
