//! The announcer, and the score.
//!
//! Something silly happens and the game says so, loudly. An [`Announce`] puts a
//! banner on the screen that springs in, holds, and floats away. [`Points`] go
//! into a fun score, and each one that follows another within a couple of
//! seconds is worth more than the last: the combo is what makes it a game.
//!
//! The score is not a lap time and never touches one. It is the number of
//! ridiculous things done in a lap, which is a thing that can be beaten, and is
//! kept per circuit.

use bevy::prelude::*;

use super::Fun;
use super::air::Landed;
use super::parts::rainbow;
use crate::car::{Car, Player};
use crate::hud::Instrument;
use crate::lap::LapFinished;
use crate::sound::{Sfx, SfxKind};
use crate::text::{t, tf};

/// The combo can climb this far.
const MOST_COMBO: u32 = 8;
/// Seconds a combo waits for the next thing to score.
const COMBO_WINDOW: f32 = 2.6;
/// A drift has to last this long to be worth points.
const DRIFT_MIN: f32 = 0.9;

/// How a banner is dressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    /// Large, in the colours of the moment.
    Big,
    /// Smaller, white, underneath.
    Small,
    /// Large and golden: for records.
    Gold,
    /// Large and red: for something about to happen to you.
    Alarm,
}

/// Put a banner up.
#[derive(Message, Clone, Debug)]
pub(crate) struct Announce {
    pub key: &'static str,
    pub arg: Option<String>,
    pub tone: Tone,
}

impl Announce {
    pub(crate) fn big(key: &'static str) -> Self {
        Self {
            key,
            arg: None,
            tone: Tone::Big,
        }
    }
    pub(crate) fn small(key: &'static str) -> Self {
        Self {
            tone: Tone::Small,
            ..Self::big(key)
        }
    }
    pub(crate) fn gold(key: &'static str) -> Self {
        Self {
            tone: Tone::Gold,
            ..Self::big(key)
        }
    }
    pub(crate) fn alarm(key: &'static str) -> Self {
        Self {
            tone: Tone::Alarm,
            ..Self::big(key)
        }
    }
    pub(crate) fn with(mut self, arg: impl ToString) -> Self {
        self.arg = Some(arg.to_string());
        self
    }
}

/// Something was worth points.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct Points {
    pub amount: u32,
    /// A key for what it was, shown beside the number.
    pub what: &'static str,
    /// Where in the world, for the number to float up from.
    pub at: Option<Vec3>,
}

/// Everything done in a lap, and what it added up to.
#[derive(Resource, Default, Clone, Debug)]
pub(crate) struct Score {
    /// This lap so far, and the lap before.
    pub lap: u64,
    pub last: u64,
    /// The best this session, on this circuit.
    pub best: u64,
    /// The multiplier, and the seconds left before it drops back to one.
    pub combo: u32,
    pub left: f32,
    /// Things done, for the awards.
    pub stats: Stats,
    /// The circuit the best belongs to.
    circuit: String,
}

/// A running count of ridiculous things done.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Stats {
    pub honks: u32,
    pub jumps: u32,
    pub airtime: f32,
    pub drift_metres: f32,
    pub boosts: u32,
    pub smashed: u32,
    pub cows: u32,
    pub strikes: u32,
    pub balloons: u32,
    pub boxes: u32,
    pub events: u32,
    pub crows: u32,
    /// The highest combo reached.
    pub combo_peak: u32,
}

impl Score {
    /// Add `amount`, at the current combo, and push the combo up. Returns what
    /// was actually scored.
    pub(crate) fn add(&mut self, amount: u32) -> u32 {
        let combo = self.combo.max(1);
        let scored = amount * combo;
        self.lap += u64::from(scored);
        self.combo = (combo + 1).min(MOST_COMBO);
        self.stats.combo_peak = self.stats.combo_peak.max(self.combo);
        self.left = COMBO_WINDOW;
        scored
    }

    /// Count what an earning of `what` was, for the awards.
    pub(crate) fn tally(&mut self, what: &str) {
        let stats = &mut self.stats;
        match what {
            "pop.cow" => stats.cows += 1,
            "pop.balloon" => stats.balloons += 1,
            "pop.box" => stats.boxes += 1,
            "pop.chaos" => stats.events += 1,
            "pop.strike" => stats.strikes += 1,
            "pop.boost" => stats.boosts += 1,
            "pop.cone" | "pop.pin" | "pop.duck" | "pop.melon" | "pop.crate" => stats.smashed += 1,
            _ => {}
        }
    }

    /// The name of the rank a lap's score has earned.
    pub(crate) fn rank(points: u64) -> &'static str {
        match points {
            0..=499 => "rank.0",
            500..=1_999 => "rank.1",
            2_000..=5_999 => "rank.2",
            6_000..=14_999 => "rank.3",
            15_000..=34_999 => "rank.4",
            _ => "rank.5",
        }
    }
}

/// The banners' home, and the score's.
#[derive(Component)]
struct BannerLayer;
#[derive(Component)]
struct ScorePanel;
#[derive(Component)]
struct ScoreNumber;
#[derive(Component)]
struct ScoreCombo;
#[derive(Component)]
struct ScoreRank;
#[derive(Component)]
struct ComboBar;

/// A banner, and how old it is.
#[derive(Component)]
struct Banner {
    age: f32,
    life: f32,
    tone: Tone,
    hue: f32,
}
/// The dark copy of a banner's words that sits behind it.
#[derive(Component)]
struct Shadow;
/// A floating number.
#[derive(Component)]
struct Pop {
    age: f32,
    from: Vec2,
}

pub(super) fn plugin(app: &mut App) {
    app.add_message::<Announce>()
        .add_message::<Points>()
        .init_resource::<Score>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                score_the_air,
                score_the_drifts,
                count_honks,
                take_points,
                new_lap,
                spawn_banners,
                animate_banners,
                animate_pops,
                draw_score,
            )
                .chain()
                .run_if(super::silly),
        )
        // Whether the panel is up is the HUD's to say, with its own rule for
        // when the game is stopped; it is put right after that, in every game,
        // silly or not, or a game that is not would show a score of nothing.
        .add_systems(Update, show_score.after(crate::hud::show))
        // And whatever was up when the fun stopped goes with it: nothing else
        // is left running to take it down.
        .add_systems(Update, clear_words.run_if(not(super::silly)));
}

/// Whether the score is showing: while there is a game to score, and it is silly.
fn show_score(
    fun: Res<Fun>,
    halt: Res<crate::pause::Halt>,
    mut panel: Query<&mut Visibility, With<ScorePanel>>,
) {
    let shown = fun.silly() && !halt.stopped();
    for mut visibility in &mut panel {
        visibility.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// A banner or a floating number.
type Words = Or<(With<Banner>, With<Pop>)>;

/// Take down every banner and floating number.
fn clear_words(mut commands: Commands, words: Query<Entity, Words>) {
    for word in &words {
        commands.entity(word).despawn();
    }
}

fn setup(mut commands: Commands) {
    use crate::ui::label;
    // The banners come down the middle of the top third.
    commands.spawn((
        BannerLayer,
        GlobalZIndex(14),
        Node {
            position_type: PositionType::Absolute,
            top: percent(15),
            width: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(2),
            ..default()
        },
        Pickable::IGNORE,
    ));
    // The score, top and centre.
    commands
        .spawn((
            ScorePanel,
            Instrument,
            Node {
                position_type: PositionType::Absolute,
                top: px(22),
                left: percent(50),
                margin: UiRect::left(px(-120)),
                width: px(240),
                padding: UiRect::axes(px(14), px(8)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(1),
                border_radius: BorderRadius::all(px(14)),
                ..default()
            },
            BackgroundColor(crate::hud::PANEL),
            Visibility::Hidden,
        ))
        .with_children(|panel| {
            panel.spawn(crate::text::label(
                "score.label",
                11.0,
                crate::hud::AMBER_DIM,
            ));
            panel
                .spawn(Node {
                    column_gap: px(10),
                    align_items: AlignItems::Baseline,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        ScoreNumber,
                        Text::new("0"),
                        TextFont {
                            font_size: FontSize::Px(34.0),
                            weight: FontWeight::EXTRA_BOLD,
                            ..default()
                        },
                        TextColor(crate::ui::TEXT),
                    ));
                    row.spawn((
                        ScoreCombo,
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(22.0),
                            weight: FontWeight::BLACK,
                            ..default()
                        },
                        TextColor(crate::ui::ACCENT),
                    ));
                });
            panel.spawn((ScoreRank, label("", 12.0, crate::hud::AMBER_DIM)));
            panel
                .spawn(Node {
                    width: percent(100),
                    height: px(4),
                    margin: UiRect::top(px(3)),
                    border_radius: BorderRadius::all(px(2)),
                    ..default()
                })
                .insert(BackgroundColor(crate::ui::LINE))
                .with_children(|bar| {
                    bar.spawn((
                        ComboBar,
                        Node {
                            width: percent(0),
                            height: percent(100),
                            border_radius: BorderRadius::all(px(2)),
                            ..default()
                        },
                        BackgroundColor(crate::ui::ACCENT),
                    ));
                });
        });
}

/// Airtime is worth points, and a big jump gets a banner.
fn score_the_air(
    mut landed: MessageReader<Landed>,
    mut points: MessageWriter<Points>,
    mut said: MessageWriter<Announce>,
    mut score: ResMut<Score>,
    cars: Query<&Transform, With<Player>>,
) {
    for down in landed.read() {
        score.stats.jumps += 1;
        score.stats.airtime += down.airtime;
        if down.airtime < 0.35 {
            continue;
        }
        let amount = (down.airtime * 140.0 + down.peak * 60.0) as u32;
        let at = cars.single().ok().map(|c| c.translation + Vec3::Y * 0.6);
        points.write(Points {
            amount,
            what: "pop.air",
            at,
        });
        if down.peak > 3.0 || down.airtime > 1.6 {
            said.write(Announce::big("say.bigair"));
        } else if down.airtime > 0.8 {
            said.write(Announce::big("say.airtime"));
        }
    }
}

/// Every honk is counted, and a crow is remembered.
fn count_honks(mut honks: MessageReader<super::mount::Honk>, mut score: ResMut<Score>) {
    for honk in honks.read() {
        score.stats.honks += 1;
        if honk.crowed {
            score.stats.crows += 1;
        }
    }
}

/// A slide is worth points once it has lasted, and is banked when it ends.
#[allow(clippy::type_complexity)]
fn score_the_drifts(
    time: Res<Time>,
    mut points: MessageWriter<Points>,
    mut score: ResMut<Score>,
    mut drift: Local<(f32, f32)>,
    cars: Query<(&Transform, &Car), With<Player>>,
) {
    let Ok((at, car)) = cars.single() else {
        return;
    };
    let speed = car.velocity.length();
    let sliding = car.rear_slip > 0.5 && speed > 6.0;
    if sliding {
        drift.0 += time.delta_secs();
        drift.1 += speed * time.delta_secs();
    } else if drift.0 > 0.0 {
        if drift.0 > DRIFT_MIN {
            score.stats.drift_metres += drift.1;
            points.write(Points {
                amount: (drift.1 * 3.0).min(2_000.0) as u32,
                what: "pop.drift",
                at: Some(at.translation + Vec3::Y * 0.5),
            });
        }
        *drift = (0.0, 0.0);
    }
}

/// Points go into the score, and floating numbers come out of the world.
#[allow(clippy::too_many_arguments)]
fn take_points(
    mut commands: Commands,
    time: Res<Time>,
    mut points: MessageReader<Points>,
    mut sounds: MessageWriter<Sfx>,
    mut said: MessageWriter<Announce>,
    mut score: ResMut<Score>,
    ui_scale: Res<UiScale>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) {
    if score.left > 0.0 {
        score.left = (score.left - time.delta_secs()).max(0.0);
        if score.left == 0.0 {
            score.combo = 0;
        }
    }
    let Ok(window) = windows.single() else {
        points.clear();
        return;
    };
    for earned in points.read() {
        let before = score.combo.max(1);
        score.tally(earned.what);
        let scored = score.add(earned.amount);
        if score.combo > before && score.combo >= 3 {
            sounds.write(
                Sfx::new(SfxKind::Chime)
                    .pitch(0.85 + 0.06 * score.combo as f32)
                    .gain(0.5),
            );
        }
        if score.combo == MOST_COMBO && before == MOST_COMBO - 1 {
            said.write(Announce::big("say.combo").with(MOST_COMBO));
        }
        // A number that floats up from where it happened.
        let Some(world) = earned.at else {
            continue;
        };
        let Ok((camera, transform)) = cameras.single() else {
            continue;
        };
        let (Ok(at), Some(view)) = (
            camera.world_to_viewport(transform, world),
            camera.logical_viewport_size(),
        ) else {
            continue;
        };
        // A window with no size to put a number in.
        if view.x < 1.0 || view.y < 1.0 {
            continue;
        }
        let on_window = Vec2::new(
            at.x / view.x * window.width(),
            at.y / view.y * window.height(),
        );
        let from = on_window / ui_scale.0.max(0.1);
        let words = format!("+{scored} {}", t(earned.what));
        commands.spawn((
            Pop { age: 0.0, from },
            GlobalZIndex(13),
            Node {
                position_type: PositionType::Absolute,
                left: px(from.x - 60.0),
                top: px(from.y),
                ..default()
            },
            Text::new(words),
            TextFont {
                font_size: FontSize::Px(26.0),
                weight: FontWeight::BLACK,
                ..default()
            },
            TextColor(Color::srgb(1.0, 0.9, 0.3)),
            Pickable::IGNORE,
        ));
    }
}

/// A finished lap: bank the score, say if it is a best.
fn new_lap(
    mut laps: MessageReader<LapFinished>,
    mut resets: MessageReader<crate::Reset>,
    track: Res<crate::track::Track>,
    mut score: ResMut<Score>,
    mut said: MessageWriter<Announce>,
    mut sounds: MessageWriter<Sfx>,
) {
    // A car put back on the grid is starting again, and so is what it scores.
    if resets.read().next().is_some() {
        score.lap = 0;
        score.combo = 0;
        score.left = 0.0;
    }
    let id = track.circuit().id;
    if score.circuit != id {
        // A different circuit: its best is its own.
        score.circuit = id.to_string();
        score.best = 0;
        score.lap = 0;
        score.last = 0;
        score.combo = 0;
    }
    if laps.read().next().is_none() {
        return;
    }
    score.last = score.lap;
    if score.lap > 0 && score.lap > score.best {
        if score.best > 0 {
            said.write(Announce::gold("say.highscore"));
            sounds.write(Sfx::new(SfxKind::Tada));
        }
        score.best = score.lap;
    }
    score.lap = 0;
}

fn spawn_banners(
    mut commands: Commands,
    mut said: MessageReader<Announce>,
    layer: Query<Entity, With<BannerLayer>>,
    live: Query<(Entity, &Banner)>,
    mut count: Local<u32>,
) {
    let Ok(layer) = layer.single() else {
        said.clear();
        return;
    };
    // The banners there are, oldest first, and kept up to date with the ones
    // this frame adds and takes down: the query knows nothing of either until
    // the commands have run, and a frame with several things to say would take
    // the same one down more than once and go over four.
    let mut all: Vec<(Entity, f32)> = live.iter().map(|(e, banner)| (e, banner.age)).collect();
    all.sort_by(|a, b| b.1.total_cmp(&a.1));
    for asked in said.read() {
        let words = match &asked.arg {
            Some(arg) => tf(asked.key, &[arg]),
            None => t(asked.key).to_string(),
        };
        // Never more than four at once: the oldest goes.
        if all.len() >= 4 {
            let (old, _) = all.remove(0);
            commands.entity(old).despawn();
        }
        *count += 1;
        let (size, life) = match asked.tone {
            Tone::Small => (30.0, 1.5),
            _ => (78.0, 1.7),
        };
        let banner = commands
            .spawn((
                Banner {
                    age: 0.0,
                    life,
                    tone: asked.tone,
                    hue: (*count as f32 * 0.137).fract(),
                },
                Node {
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                UiTransform::from_scale(Vec2::splat(0.2)),
                Pickable::IGNORE,
            ))
            .with_children(|b| {
                let font = TextFont {
                    font_size: FontSize::Px(size),
                    weight: FontWeight::BLACK,
                    ..default()
                };
                b.spawn((
                    Shadow,
                    Text::new(words.clone()),
                    font.clone(),
                    TextColor(Color::srgba(0.02, 0.0, 0.08, 0.7)),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(size * 0.05),
                        top: px(size * 0.06),
                        ..default()
                    },
                ));
                b.spawn((Text::new(words), font, TextColor(Color::WHITE)));
            })
            .id();
        commands.entity(layer).add_child(banner);
        all.push((banner, 0.0));
    }
}

/// Spring in, hold, float away.
fn animate_banners(
    mut commands: Commands,
    time: Res<Time<Real>>,
    fun: Res<Fun>,
    mut banners: Query<(Entity, &mut Banner, &mut UiTransform, &Children)>,
    mut colours: Query<(&mut TextColor, Option<&Shadow>)>,
) {
    let dt = time.delta_secs();
    for (entity, mut banner, mut transform, children) in &mut banners {
        banner.age += dt;
        let age = banner.age;
        if age >= banner.life {
            commands.entity(entity).despawn();
            continue;
        }
        // Overshoot and settle, the way cartoon type does.
        let pop = 0.22;
        let scale = if age < pop {
            let t = age / pop;
            0.2 + 1.05 * (t * (2.0 - t))
        } else {
            let t = ((age - pop) / 0.2).min(1.0);
            1.25 - 0.25 * t * t * (3.0 - 2.0 * t)
        };
        let fade = ((banner.life - age) / 0.35).clamp(0.0, 1.0);
        let wobble = if fun.calm {
            0.0
        } else {
            (age * 11.0).sin() * 2.5 * (1.0 - (age / banner.life))
        };
        // For anyone who would rather nothing sprang or floated: it appears, at its
        // size, and fades.
        let (scale, float) = if fun.calm {
            (1.0, 0.0)
        } else {
            (scale, -(1.0 - fade) * 36.0)
        };
        transform.scale = Vec2::splat(scale);
        transform.rotation = Rot2::degrees(wobble);
        transform.translation = Val2::px(0.0, float);
        let base = match banner.tone {
            Tone::Big => rainbow(banner.hue + age * if fun.calm { 0.0 } else { 0.7 }, 0.62),
            Tone::Small => Color::WHITE,
            Tone::Gold => Color::srgb(1.0, 0.82, 0.18),
            Tone::Alarm => Color::srgb(1.0, 0.26, 0.28),
        };
        for child in children.iter() {
            if let Ok((mut colour, shadow)) = colours.get_mut(child) {
                colour.0 = if shadow.is_some() {
                    Color::srgba(0.02, 0.0, 0.08, 0.7 * fade)
                } else {
                    base.with_alpha(fade)
                };
            }
        }
    }
}

/// Numbers float up and go.
fn animate_pops(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut pops: Query<(Entity, &mut Pop, &mut Node, &mut TextColor)>,
) {
    for (entity, mut pop, mut node, mut colour) in &mut pops {
        pop.age += time.delta_secs();
        if pop.age > 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let rise = pop.age * 70.0 - pop.age * pop.age * 30.0;
        node.top = px(pop.from.y - rise);
        colour.0 = Color::srgb(1.0, 0.9, 0.3).with_alpha((1.0 - pop.age).clamp(0.0, 1.0).sqrt());
    }
}

/// A number with its thousands set apart the way the language in force does it: a
/// comma, a point, or a space that does not break.
pub(crate) fn thousands(n: u64) -> String {
    thousands_in(crate::text::current(), n)
}

fn thousands_in(language: crate::text::Language, n: u64) -> String {
    use crate::text::Language;
    let apart = match language {
        Language::En => ',',
        Language::De | Language::It => '.',
        Language::Fr | Language::Es => '\u{00A0}',
    };
    let digits = n.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(apart);
        }
        out.push(digit);
    }
    out
}

/// The score panel: the number, the combo, the rank and the timer.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw_score(
    score: Res<Score>,
    halt: Res<crate::pause::Halt>,
    mut number: Query<&mut Text, (With<ScoreNumber>, Without<ScoreCombo>, Without<ScoreRank>)>,
    mut combo: Query<
        (&mut Text, &mut TextColor),
        (With<ScoreCombo>, Without<ScoreNumber>, Without<ScoreRank>),
    >,
    mut rank: Query<&mut Text, (With<ScoreRank>, Without<ScoreNumber>, Without<ScoreCombo>)>,
    mut bar: Query<&mut Node, With<ComboBar>>,
) {
    if halt.stopped() {
        return;
    }
    if let Ok(mut text) = number.single_mut() {
        let wanted = thousands(score.lap);
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
    if let Ok((mut text, mut colour)) = combo.single_mut() {
        let wanted = if score.combo > 1 {
            format!("×{}", score.combo)
        } else {
            String::new()
        };
        if text.0 != wanted {
            text.0 = wanted;
        }
        colour.0 = rainbow(score.combo as f32 * 0.11, 0.6);
    }
    if let Ok(mut text) = rank.single_mut() {
        let wanted = t(Score::rank(score.lap));
        if text.0 != wanted {
            text.0 = wanted.into();
        }
    }
    if let Ok(mut node) = bar.single_mut() {
        node.width = percent(100.0 * (score.left / COMBO_WINDOW).clamp(0.0, 1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_combo_multiplies_what_follows_it_and_stops_at_the_top() {
        let mut score = Score::default();
        assert_eq!(score.add(100), 100, "the first thing is worth what it says");
        assert_eq!(score.add(100), 200);
        assert_eq!(score.add(100), 300);
        for _ in 0..20 {
            score.add(1);
        }
        assert_eq!(score.combo, MOST_COMBO);
        assert_eq!(score.add(100), 100 * MOST_COMBO, "and no further");
        assert!(score.left > COMBO_WINDOW - 0.01);
    }

    #[test]
    fn the_score_is_shown_only_in_a_silly_game_whatever_the_hud_says() {
        use crate::pause::Halt;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Halt::Title)
            .init_resource::<Fun>()
            .add_systems(Update, (crate::hud::show, show_score).chain());
        let panel = app
            .world_mut()
            .spawn((ScorePanel, Instrument, Visibility::Hidden))
            .id();
        for (level, shown) in [
            (super::super::Silliness::Serious, false),
            (super::super::Silliness::Silly, true),
            (super::super::Silliness::Bonkers, true),
            (super::super::Silliness::Serious, false),
        ] {
            app.world_mut().resource_mut::<Fun>().level = level;
            // The game is stopped, and then it starts, which is when the HUD
            // makes every one of its instruments visible.
            *app.world_mut().resource_mut::<Halt>() = Halt::Title;
            app.update();
            assert_eq!(
                *app.world().get::<Visibility>(panel).unwrap(),
                Visibility::Hidden,
                "{level:?} stopped"
            );
            *app.world_mut().resource_mut::<Halt>() = Halt::Nothing;
            app.update();
            assert_eq!(
                *app.world().get::<Visibility>(panel).unwrap() != Visibility::Hidden,
                shown,
                "{level:?} driving"
            );
        }
    }

    #[test]
    fn what_was_on_screen_when_the_fun_stopped_is_taken_down() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Fun>()
            .add_systems(Update, clear_words.run_if(not(super::super::silly)));
        let banner = app
            .world_mut()
            .spawn(Banner {
                age: 0.1,
                life: 1.7,
                tone: Tone::Big,
                hue: 0.0,
            })
            .id();
        let pop = app
            .world_mut()
            .spawn(Pop {
                age: 0.1,
                from: Vec2::ZERO,
            })
            .id();
        // In a silly game they are left to animate themselves away.
        app.world_mut().resource_mut::<Fun>().level = super::super::Silliness::Silly;
        app.update();
        assert!(app.world().get_entity(banner).is_ok() && app.world().get_entity(pop).is_ok());
        // Not silly: nothing is left running to do it, so this is what does.
        app.world_mut().resource_mut::<Fun>().level = super::super::Silliness::Serious;
        app.update();
        assert!(app.world().get_entity(banner).is_err() && app.world().get_entity(pop).is_err());
    }

    #[test]
    fn a_frame_with_a_lot_to_say_still_shows_four_at_most() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Announce>()
            .add_systems(Update, spawn_banners);
        app.world_mut().spawn((BannerLayer, Node::default()));
        for _ in 0..7 {
            app.world_mut().write_message(Announce::big("say.airtime"));
        }
        app.update();
        let count = |app: &mut App| {
            let world = app.world_mut();
            world.query::<&Banner>().iter(world).count()
        };
        assert_eq!(count(&mut app), 4, "seven at once");
        app.world_mut().write_message(Announce::big("say.bigair"));
        app.world_mut().write_message(Announce::big("say.bigair"));
        app.update();
        assert_eq!(count(&mut app), 4, "and two more on the next frame");
    }

    #[test]
    fn a_car_put_back_on_the_grid_starts_its_score_again() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LapFinished>()
            .add_message::<crate::Reset>()
            .add_message::<Announce>()
            .add_message::<Sfx>()
            .init_resource::<Score>()
            .insert_resource(crate::track::Track::any())
            .add_systems(Update, new_lap);
        app.update();
        {
            let mut score = app.world_mut().resource_mut::<Score>();
            score.add(400);
            score.add(400);
        }
        app.update();
        assert!(app.world().resource::<Score>().lap > 0, "not by itself");
        app.world_mut().write_message(crate::Reset);
        app.update();
        let score = app.world().resource::<Score>();
        assert_eq!((score.lap, score.combo), (0, 0));
    }

    #[test]
    fn a_number_is_set_out_the_way_each_language_writes_it() {
        use crate::text::Language::*;
        assert_eq!(thousands_in(En, 1_234_567), "1,234,567");
        assert_eq!(thousands_in(De, 1_234_567), "1.234.567");
        assert_eq!(thousands_in(It, 12_345), "12.345");
        assert_eq!(thousands_in(Fr, 1_234_567), "1\u{00A0}234\u{00A0}567");
        assert_eq!(thousands_in(Es, 12_345), "12\u{00A0}345");
        for language in [En, De, Fr, Es, It] {
            assert_eq!(thousands_in(language, 0), "0");
            assert_eq!(thousands_in(language, 999), "999");
            assert_eq!(thousands_in(language, 100_000).chars().count(), 7);
        }
    }

    #[test]
    fn the_ranks_climb_and_every_one_is_named() {
        let ranks: Vec<_> = [0u64, 499, 500, 2_000, 6_000, 15_000, 35_000, 1_000_000]
            .iter()
            .map(|p| Score::rank(*p))
            .collect();
        assert_eq!(
            ranks,
            [
                "rank.0", "rank.0", "rank.1", "rank.2", "rank.3", "rank.4", "rank.5", "rank.5"
            ]
        );
    }

    #[test]
    fn the_combo_runs_out_when_nothing_scores() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Score>()
            .add_message::<Points>()
            .add_message::<Announce>()
            .add_message::<Sfx>()
            .init_resource::<UiScale>()
            .add_systems(Update, take_points);
        app.world_mut()
            .spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut().write_message(Points {
            amount: 50,
            what: "pop.air",
            at: None,
        });
        app.update();
        assert_eq!(app.world().resource::<Score>().lap, 50);
        assert!(app.world().resource::<Score>().combo > 1);
        app.world_mut().resource_mut::<Score>().left = 0.001;
        std::thread::sleep(std::time::Duration::from_millis(30));
        app.update();
        app.update();
        assert_eq!(
            app.world().resource::<Score>().combo,
            0,
            "the combo dropped"
        );
    }

    #[test]
    fn an_announcement_carries_its_words() {
        let a = Announce::big("say.combo").with(8);
        assert_eq!(a.arg.as_deref(), Some("8"));
        assert_eq!(a.tone, Tone::Big);
        assert_eq!(Announce::alarm("x").tone, Tone::Alarm);
        assert_eq!(Announce::gold("x").tone, Tone::Gold);
        assert_eq!(Announce::small("x").tone, Tone::Small);
    }
}
