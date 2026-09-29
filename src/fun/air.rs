//! Leaving the ground.
//!
//! The engine keeps the car on the road: [`crate::track::Track::hold`] sets its
//! height from the ground every step and nothing in it knows what air is. That
//! is left exactly as it is. What this layer adds is a height *above* the road,
//! carried beside the car in an [`Air`], that the model rides on and the camera
//! follows. The car underneath keeps driving along the ground; the thing on top
//! of it is somewhere else.
//!
//! **Crests launch it.** A car that drives over the top of a hill leaves the
//! ground exactly when a thrown stone would: when the road falls away from its
//! path faster than gravity pulls a thing down, which is when the road's
//! vertical acceleration under the car, `v²κ`, is more than `g`. So nobody
//! decides where the jumps are: the hills do, and the wilder the hills and the
//! faster the car, the more of them there are. The road's height is read off the
//! centreline, which is smooth, and not off the ground under the wheels, which
//! has a kerb in it.
//!
//! **Gravity scales with the speed.** A car `k` times as fast is asked to make
//! the same jumps in `1/k` of the time, which is `k²` times the gravity. The
//! shape of a jump in space is then the shape it always was.
//!
//! The arithmetic is a plain function of the road and the clock so that it can
//! be tested against roads made up for the purpose.

use bevy::prelude::*;

use super::{Fun, Mount};
use crate::car::{Car, Controls, DriveSet, Player, step_seconds};
use crate::settings::bindings::{Act, current};
use crate::track::Track;

/// A little less gravity than the world's, so jumps hang a moment.
pub(crate) const FLOATINESS: f32 = 0.72;
/// A hop's launch speed, in metres a second at the shipped car's pace.
const HOP: f32 = 4.4;
/// Seconds after landing before the next crest can launch.
const SETTLE: f32 = 0.18;
/// How long a hop pressed a moment too early is remembered.
const BUFFER: f32 = 0.16;
/// A chicken that holds the hop key falls this much more slowly.
const GLIDE: f32 = 0.32;

/// The player's car, when it is being driven and not held on the grid.
type Driven = (With<Player>, Without<crate::countdown::Held>);

/// How far off the road a car is, and what it did on the way down.
#[derive(Component, Default, Clone, Copy, Debug)]
pub(crate) struct Air {
    /// Whether the car has left the road.
    pub flying: bool,
    /// Height above the road in game units. Zero on the ground.
    pub height: f32,
    /// Vertical speed of the car's body, up positive, in game units a second.
    pub vy: f32,
    /// How hard the last landing was, until the pose has used it.
    pub landed: f32,
    /// Seconds into this flight, and how high it has been.
    pub airtime: f32,
    pub peak: f32,
    /// Hop keys pressed and not yet acted on, as seconds of memory left.
    pub asked: f32,
    /// Seconds before another launch from a crest.
    settle: f32,
    /// The body's height in the world while it flies.
    y: f32,
    /// The road under the car: its height, and how fast and how hard that is
    /// changing, smoothed over a few steps.
    road_y: f32,
    road_vy: f32,
    road_ay: f32,
    road_vy_raw: f32,
    /// Looks taken at the road so far, up to three: one to know where it is,
    /// a second to know how fast it is rising, and only then how that changes.
    looks: u8,
}

/// What happened this step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Event {
    Launched {
        power: f32,
    },
    Landed {
        impact: f32,
        airtime: f32,
        peak: f32,
    },
}

/// The car has left the road, on purpose or by the hills.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct Launched {
    pub power: f32,
}

/// The car has come down.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct Landed {
    pub impact: f32,
    pub airtime: f32,
    pub peak: f32,
}

impl Air {
    /// Throw the body into the air at `vy`, from `ground_y`.
    pub(crate) fn launch(&mut self, ground_y: f32, vy: f32) {
        self.flying = true;
        self.y = ground_y;
        self.vy = vy;
        self.height = 0.0;
        self.airtime = 0.0;
        self.peak = 0.0;
        self.asked = 0.0;
    }

    /// Forget where the road was: the car has been put somewhere else.
    pub(crate) fn lost(&mut self) {
        *self = Self::default();
    }

    /// One step. `road_y` is the height of the centreline under the car,
    /// `ground_y` the height of the ground it stands on, `speed` its speed over
    /// the ground and `g` gravity, in the units of this speed.
    pub(crate) fn step(
        &mut self,
        dt: f32,
        g: f32,
        road_y: f32,
        ground_y: f32,
        speed: f32,
        min_speed: f32,
    ) -> Option<Event> {
        // The road under the car: how fast it rises, and how fast that changes.
        if self.looks == 0 || (road_y - self.road_y).abs() > 1.5 {
            // The first look, or the car has been carried somewhere: not a hill.
            self.looks = 1;
            self.road_y = road_y;
            self.road_vy = 0.0;
            self.road_vy_raw = 0.0;
            self.road_ay = 0.0;
        } else {
            let raw = (road_y - self.road_y) / dt;
            self.road_y = road_y;
            if self.looks == 1 {
                // The second look says how fast the road is rising, and nothing
                // yet about how that is changing: the road it was rising at
                // before this is not known, and a guess of nothing would read as
                // a cliff.
                self.looks = 2;
                self.road_vy = raw;
            } else {
                let accel = (raw - self.road_vy_raw) / dt;
                self.road_vy += (raw - self.road_vy) * 0.30;
                self.road_ay += (accel - self.road_ay) * 0.12;
            }
            self.road_vy_raw = raw;
        }
        self.settle = (self.settle - dt).max(0.0);

        if !self.flying {
            self.height = 0.0;
            if self.settle <= 0.0 && speed > min_speed && self.road_ay < -g {
                // The road has fallen away faster than a stone would: it goes on
                // up, at the speed the road was rising, and gravity takes over.
                self.launch(ground_y, self.road_vy);
                return Some(Event::Launched { power: 0.0 });
            }
            return None;
        }

        self.vy -= g * dt;
        self.y += self.vy * dt;
        self.airtime += dt;
        self.height = (self.y - ground_y).max(0.0);
        self.peak = self.peak.max(self.height);
        if self.y <= ground_y {
            // Down: as hard as the body was falling relative to the road.
            let impact = (-(self.vy - self.road_vy)).max(0.0);
            let event = Event::Landed {
                impact,
                airtime: self.airtime,
                peak: self.peak,
            };
            self.flying = false;
            self.height = 0.0;
            self.vy = 0.0;
            self.landed = impact;
            self.settle = SETTLE;
            return Some(event);
        }
        None
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_message::<Launched>()
        .add_message::<Landed>()
        .add_systems(Update, (attach, ask))
        .add_systems(PreUpdate, forget.after(crate::car::CarResetSet))
        .add_systems(FixedUpdate, (tamper.before(DriveSet), fly.after(DriveSet)));
}

/// Give the player's car somewhere to keep its height off the road.
fn attach(mut commands: Commands, cars: Query<Entity, (With<Player>, Without<Air>)>) {
    for car in &cars {
        commands.entity(car).insert(Air::default());
    }
}

/// A car put back on the grid, or fetched back to the road, is not in the air.
fn forget(mut resets: MessageReader<crate::Reset>, mut cars: Query<&mut Air>) {
    if resets.read().next().is_some() {
        for mut air in &mut cars {
            air.lost();
        }
    }
}

/// The hop key, noticed on a frame and acted on at the next physics step. Not
/// while the game is stopped: the key is also the next tab in the settings and the
/// next view on the board, and a hop asked for there would wait, with the clock
/// that counts it down stopped too, for the first step after the game went on.
fn ask(
    time: Res<Time>,
    halt: Res<crate::pause::Halt>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    settings: Option<Res<crate::settings::Settings>>,
    fun: Res<Fun>,
    mut cars: Query<&mut Air, With<Player>>,
) {
    for mut air in &mut cars {
        if halt.stopped() {
            air.asked = 0.0;
            continue;
        }
        air.asked = (air.asked - time.delta_secs()).max(0.0);
        if fun.bonkers() && current(settings.as_deref()).just(&keys, &pads, Act::Hop) {
            air.asked = BUFFER;
        }
    }
}

/// In the air the driver has nothing to steer with.
fn tamper(mut cars: Query<(&mut Controls, &Air), With<Player>>) {
    for (mut controls, air) in &mut cars {
        if air.flying {
            *controls = Controls::default();
        }
    }
}

/// Fly, launch and land.
#[allow(clippy::too_many_arguments)]
fn fly(
    fun: Res<Fun>,
    track: Res<Track>,
    race: Option<Res<crate::local::LocalRace>>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    settings: Option<Res<crate::settings::Settings>>,
    moon: Option<Res<super::events::Gravity>>,
    mut cars: Query<(&Transform, &Car, &mut Air), Driven>,
    mut launched: MessageWriter<Launched>,
    mut landed: MessageWriter<Landed>,
) {
    let active = fun.bonkers() && crate::local::solo(race);
    let k = fun.speed.scale();
    let dt = step_seconds();
    let low = moon.map_or(1.0, |m| m.factor());
    for (at, car, mut air) in &mut cars {
        if !active {
            if air.flying || air.height > 0.0 {
                air.lost();
            }
            continue;
        }
        let ground = track.ground_from(at.translation, car.along);
        let held = current(settings.as_deref()).key_held(&keys, Act::Hop)
            || current(settings.as_deref())
                .pad(Act::Hop)
                .is_some_and(|b| pads.iter().any(|pad| pad.pressed(b)));
        // A chicken that keeps hold of the hop key and is on its way down
        // flaps, and does not fall so fast.
        let glide = fun.mount == Mount::Chicken && held && air.flying && air.vy < 0.5;
        let g = 9.81 * FLOATINESS * k * k * low * if glide { GLIDE } else { 1.0 };
        let speed = car.velocity.length();
        let event = air.step(dt, g, ground.centre.y, at.translation.y, speed, 4.0 * k);
        if !air.flying && air.asked > 0.0 {
            // A hop, from the ground.
            air.launch(at.translation.y, HOP * k);
            launched.write(Launched { power: 1.0 });
        }
        match event {
            Some(Event::Launched { power }) => {
                launched.write(Launched { power });
            }
            Some(Event::Landed {
                impact,
                airtime,
                peak,
            }) => {
                landed.write(Landed {
                    impact,
                    airtime,
                    peak,
                });
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 240.0;
    const G: f32 = 9.81 * FLOATINESS;

    /// Drive a car at `speed` along `road`, a height by distance, and report
    /// every event and the ground it was on.
    fn drive(road: impl Fn(f32) -> f32, speed: f32, seconds: f32) -> Vec<(f32, Event)> {
        let mut air = Air::default();
        let mut events = Vec::new();
        let steps = (seconds / DT) as usize;
        for i in 0..steps {
            let s = speed * i as f32 * DT;
            let y = road(s);
            if let Some(event) = air.step(DT, G, y, y, speed, 4.0) {
                events.push((s, event));
            }
        }
        events
    }

    #[test]
    fn a_flat_road_and_a_steady_slope_never_launch_anything() {
        assert!(drive(|_| 0.0, 30.0, 6.0).is_empty());
        assert!(
            drive(|s| s * 0.12, 30.0, 6.0).is_empty(),
            "a climb is not a jump"
        );
        assert!(
            drive(|s| -s * 0.2, 30.0, 6.0).is_empty(),
            "nor is a descent"
        );
    }

    #[test]
    fn a_gentle_crest_is_driven_over_and_a_sharp_one_is_left() {
        // A hill 40 m long: height = A sin(pi s / 40). At speed v the road's
        // vertical acceleration on top is A (pi v / 40)^2.
        let hill = |a: f32| {
            move |s: f32| {
                if (0.0..40.0).contains(&(s - 30.0)) {
                    a * ((s - 30.0) * std::f32::consts::PI / 40.0).sin()
                } else {
                    0.0
                }
            }
        };
        let v = 24.0;
        let limit = G / (std::f32::consts::PI * v / 40.0).powi(2);
        let gentle = drive(hill(limit * 0.6), v, 4.0);
        assert!(
            gentle.is_empty(),
            "a hill at 60% of the limit launched: {gentle:?}"
        );
        let sharp = drive(hill(limit * 1.6), v, 4.0);
        assert!(
            matches!(sharp.first(), Some((_, Event::Launched { .. }))),
            "a hill at 160% of the limit did not: {sharp:?}"
        );
        assert!(matches!(sharp.last(), Some((_, Event::Landed { .. }))));
    }

    #[test]
    fn a_launch_lands_where_ballistics_says() {
        // A ramp that stops: up at 15%, then flat ground. A car at 30 m/s leaves
        // it with vy = 0.15 * 30 and comes down 2 vy / g seconds later.
        let ramp = |s: f32| if s < 40.0 { s * 0.15 } else { 6.0 };
        let v = 30.0;
        let events = drive(ramp, v, 8.0);
        let launch = events
            .iter()
            .find(|e| matches!(e.1, Event::Launched { .. }));
        assert!(launch.is_some());
        // The road after the lip is level with the lip, so a free body climbs
        // then falls back to the same height: time of flight 2 vy / g.
        let Some((_, Event::Landed { airtime, .. })) = events
            .iter()
            .find(|e| matches!(e.1, Event::Landed { .. }))
            .copied()
        else {
            panic!("it never came down: {events:?}");
        };
        let expected = 2.0 * (0.15 * v) / G;
        assert!(
            (airtime - expected).abs() < expected * 0.35,
            "in the air for {airtime:.2} s, ballistics says {expected:.2} s"
        );
    }

    #[test]
    fn a_car_carried_somewhere_else_is_not_a_launch() {
        let mut air = Air::default();
        for i in 0..200 {
            air.step(DT, G, 0.0, 0.0, 30.0, 4.0);
            let _ = i;
        }
        // A rescue puts it 30 m down the road, and 9 m higher.
        for _ in 0..50 {
            assert_eq!(air.step(DT, G, 9.0, 9.0, 30.0, 4.0), None);
        }
        assert!(!air.flying);
    }

    #[test]
    fn a_slow_car_on_a_hump_stays_on_the_road() {
        // The same sharp hill, walked over.
        let hill = |s: f32| {
            if (0.0..6.0).contains(&(s - 3.0)) {
                1.5 * ((s - 3.0) * std::f32::consts::PI / 6.0).sin()
            } else {
                0.0
            }
        };
        assert!(drive(hill, 3.0, 6.0).is_empty());
    }

    #[test]
    fn a_hop_asked_for_in_a_menu_is_not_saved_for_afterwards() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(crate::pause::Halt::Settings)
            .insert_resource(Fun::of(&crate::settings::Settings::default()))
            .add_systems(Update, ask);
        let car = app.world_mut().spawn((Player, Air::default())).id();
        let asked = |app: &App| app.world().get::<Air>(car).unwrap().asked;
        // E is the next tab on the settings page, among other things.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        assert_eq!(asked(&app), 0.0, "a hop was kept from the settings page");
        // The game goes on with nothing waiting for it.
        app.insert_resource(crate::pause::Halt::Nothing);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.update();
        assert_eq!(asked(&app), 0.0);
        // A press while driving is a hop.
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::KeyE);
            keys.clear();
            keys.press(KeyCode::KeyE);
        }
        app.update();
        assert!(asked(&app) > 0.0);
    }

    #[test]
    fn a_hop_goes_up_and_comes_back_down() {
        let mut air = Air::default();
        air.launch(0.0, 4.4);
        let mut peak = 0.0f32;
        let mut down = None;
        for i in 0..2_400 {
            if let Some(Event::Landed { impact, .. }) = air.step(DT, G, 0.0, 0.0, 10.0, 4.0) {
                down = Some((i, impact));
                break;
            }
            peak = peak.max(air.height);
        }
        let (at, impact) = down.expect("it landed");
        assert!(peak > 0.5 && peak < 2.0, "peak {peak}");
        // Up and down again in 2 vy / g.
        assert!(((at as f32 * DT) - 2.0 * 4.4 / G).abs() < 0.08);
        assert!((impact - 4.4).abs() < 0.3, "impact {impact}");
        assert!(!air.flying && air.height == 0.0 && air.landed > 0.0);
    }
}
