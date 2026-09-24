//! The card that says how a lap went.
//!
//! Laps run straight on into the next, so this is a card rather than a
//! screen: it rises at the bottom of the screen for five seconds when a lap is
//! finished and never takes a key. The headline is the time and what it meant —
//! a new best, the gap to the best, or that it did not count and why — then
//! every sector in its colour, then the top speed and the slowest point of the
//! lap. It is built once and only filled in when a lap ends.

use bevy::prelude::*;

use crate::hud::{AMBER, AMBER_DIM, FRONT};
use crate::lap::{LapFinished, LapReport, LapSet, LapTimer, Split, Why, format_time};
use crate::pause::Halt;
use crate::settings::{Settings, Units};
use crate::ui::{LINE, TEXT, label};

/// How long the card stays up, on the game's clock.
const SHOWN_FOR: f32 = 5.0;
/// The most sectors a circuit has, and so the sector spans built.
const SPANS: usize = 8;
const RED: Color = crate::ui::Palette::STANDARD.behind;

pub struct SummaryPlugin;

impl Plugin for SummaryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Card>()
            .init_resource::<Celebration>()
            .add_systems(Startup, (setup, setup_banner))
            .add_systems(FixedUpdate, hear.after(LapSet))
            .add_systems(Update, (draw, celebrate).chain());
    }
}

/// Seconds the card has left, and whether it needs filling in.
#[derive(Resource, Default)]
pub(crate) struct Card {
    left: f32,
    fresh: bool,
}

impl Card {
    /// Hold the card up with `report` on it, for a visual check.
    #[cfg(feature = "visual-check")]
    pub(crate) fn hold(&mut self, timer: &mut LapTimer, report: LapReport) {
        if timer.report.as_ref() != Some(&report) {
            timer.report = Some(report);
            self.fresh = true;
        }
        self.left = SHOWN_FOR;
    }

    /// A lap of each kind the card shows, for a visual check.
    #[cfg(feature = "visual-check")]
    pub(crate) fn example(kind: &str) -> LapReport {
        let best = LapReport {
            number: 7,
            time: 73.6,
            valid: true,
            best: true,
            previous_best: Some(74.02),
            sectors: vec![11.02, 12.4, 9.87, 10.31, 14.2],
            splits: vec![
                Split::Purple,
                Split::Green,
                Split::Yellow,
                Split::Green,
                Split::Green,
            ],
            why: None,
            assisted: false,
            top_speed: 21.4,
            slowest: Some((7.8, 4)),
        };
        match kind {
            "valid" => LapReport {
                best: false,
                time: 74.2,
                splits: vec![
                    Split::Green,
                    Split::Yellow,
                    Split::Yellow,
                    Split::Green,
                    Split::Yellow,
                ],
                ..best
            },
            "invalid" => LapReport {
                valid: false,
                best: false,
                why: Some(Why::OffTrack { sector: 3 }),
                splits: vec![
                    Split::Green,
                    Split::Yellow,
                    Split::Plain,
                    Split::Plain,
                    Split::Plain,
                ],
                ..best
            },
            _ => best,
        }
    }
}

#[derive(Component)]
struct Panel;
#[derive(Component)]
struct Headline;
#[derive(Component)]
struct Detail;
#[derive(Component)]
struct Sectors;
/// The medal the lap earned, or what the next one asks for.
#[derive(Component)]
struct MedalLine;
#[derive(Component)]
struct SectorSpan(usize);

/// The first line: which lap, its time, and what it meant.
pub(crate) fn headline(report: &LapReport) -> String {
    use crate::text::{t, tf};
    let time = format_time(report.time);
    let meaning = if !report.valid {
        t("card.invalid").to_string()
    } else if report.best {
        match report.previous_best {
            Some(before) => format!("{}  {}", t("card.new_best"), signed(report.time - before)),
            None => t("card.first").into(),
        }
    } else {
        match report.previous_best {
            Some(best) => tf("card.to_best", &[&signed(report.time - best)]),
            None => String::new(),
        }
    };
    let assisted = if report.assisted {
        format!("    {}", t("card.assisted"))
    } else {
        String::new()
    };
    format!(
        "{} {}    {time}    {meaning}{assisted}",
        t("card.lap"),
        report.number
    )
    .trim_end()
    .to_string()
}

/// Seconds with their sign, the way a delta reads: −0.42, +1.08.
fn signed(seconds: f32) -> String {
    if seconds < 0.0 {
        format!("−{:.2}", -seconds)
    } else {
        format!("+{seconds:.2}")
    }
}

/// Why a lap did not count, in words.
pub(crate) fn reason(why: Option<Why>) -> String {
    use crate::text::{t, tf};
    match why {
        Some(Why::OffTrack { sector }) => tf("card.off_track", &[&sector]),
        Some(Why::Rescued { sector }) => tf("card.rescued", &[&sector]),
        Some(Why::Paused) => t("card.paused").into(),
        None => t("card.not_counted").into(),
    }
}

/// The last line: how fast and how slow, in the chosen units.
pub(crate) fn speeds(report: &LapReport, units: Units) -> String {
    let unit = match units {
        Units::Kmh => "km/h",
        Units::Mph => "mph",
    };
    use crate::text::tf;
    let shown = |speed| format!("{:.0}", crate::hud::displayed_speed(speed, units));
    let top = tf("card.top", &[&shown(report.top_speed), &unit]);
    match report.slowest {
        Some((speed, sector)) => tf("card.slowest", &[&top, &shown(speed), &unit, &sector]),
        None => top,
    }
}

fn split_colour(split: Split, palette: crate::ui::Palette) -> Color {
    crate::hud::split_colour(split, palette).unwrap_or(TEXT)
}

fn setup(mut commands: Commands) {
    commands
        .spawn((
            Panel,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(24),
                left: px(0),
                right: px(0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    padding: UiRect::axes(px(22), px(14)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(12)),
                    ..default()
                },
                BackgroundColor(FRONT),
                BorderColor::all(LINE),
            ))
            .with_children(|card| {
                card.spawn((Headline, label("", 24.0, TEXT)));
                card.spawn((Sectors, label("", 16.0, TEXT)))
                    .with_children(|line| {
                        for i in 0..SPANS {
                            line.spawn((
                                SectorSpan(i),
                                TextSpan::new(""),
                                TextFont {
                                    font_size: bevy::text::FontSize::Px(16.0),
                                    ..default()
                                },
                                TextColor(TEXT),
                            ));
                        }
                    });
                card.spawn((Detail, label("", 15.0, AMBER_DIM)));
                card.spawn((MedalLine, label("", 15.0, TEXT)));
            });
        });
}

/// A lap has ended: put the card up, and celebrate it if it beat the best.
fn hear(
    mut laps: MessageReader<LapFinished>,
    timer: Res<LapTimer>,
    mut card: ResMut<Card>,
    mut party: ResMut<Celebration>,
    mut cues: MessageWriter<crate::sound::Cue>,
) {
    if laps.read().last().is_some() {
        card.left = SHOWN_FOR;
        card.fresh = true;
        if timer.report.as_ref().is_some_and(celebrates) {
            party.left = CELEBRATE_FOR;
            party.fresh = true;
            cues.write(crate::sound::Cue::Best);
            cues.write(crate::sound::Cue::Cheer);
        }
    }
}

/// How long the banner stays and the clock flashes.
const CELEBRATE_FOR: f32 = 2.4;
/// The clock flashes gold this many times, at this pace.
const FLASHES: f32 = 3.0;
const FLASH_FOR: f32 = 1.2;

/// A lap is celebrated when it beat a best that already stood. The first lap
/// on a circuit is a best too, but beating nothing is not worth a fanfare.
pub(crate) fn celebrates(report: &LapReport) -> bool {
    report.valid && report.best && report.previous_best.is_some()
}

/// Whether the clock is lit gold, `left` seconds before the celebration ends.
fn flash_on(left: f32) -> bool {
    let into = CELEBRATE_FOR - left;
    into < FLASH_FOR && (into / FLASH_FOR * FLASHES).fract() < 0.5
}

#[derive(Resource, Default)]
pub(crate) struct Celebration {
    left: f32,
    fresh: bool,
}

impl Celebration {
    /// Hold the banner up, the clock lit, for a visual check.
    #[cfg(feature = "visual-check")]
    pub(crate) fn hold(&mut self) {
        if self.left == 0.0 {
            self.fresh = true;
        }
        self.left = CELEBRATE_FOR - 0.05;
    }
}

#[derive(Component)]
struct Banner;
#[derive(Component)]
struct BannerTime;

fn setup_banner(mut commands: Commands) {
    commands
        .spawn((
            Banner,
            Node {
                position_type: PositionType::Absolute,
                top: px(110),
                left: px(0),
                right: px(0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|banner| {
            banner
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(px(34), px(14)),
                        border: UiRect::all(px(2)),
                        border_radius: BorderRadius::all(px(18)),
                        ..default()
                    },
                    BackgroundColor(FRONT),
                    BorderColor::all(AMBER),
                ))
                .with_children(|panel| {
                    panel.spawn(crate::text::label("card.banner", 44.0, AMBER));
                    panel.spawn((BannerTime, label("", 26.0, TEXT)));
                });
        });
}

/// The banner, the flashing clock, and nothing at all once it is over.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn celebrate(
    time: Res<Time>,
    settings: Option<Res<Settings>>,
    halt: Res<Halt>,
    timer: Res<LapTimer>,
    mut party: ResMut<Celebration>,
    mut banners: Query<&mut Visibility, With<Banner>>,
    mut times: Query<&mut Text, With<BannerTime>>,
    mut clocks: Query<&mut TextColor, With<crate::hud::ClockReadout>>,
) {
    let was = party.left;
    party.left = (party.left - time.delta_secs()).max(0.0);
    if was == 0.0 && !party.fresh {
        return;
    }
    let showing = party.left > 0.0 && !halt.stopped();
    for mut visibility in &mut banners {
        visibility.set_if_neq(if showing {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    if party.fresh {
        party.fresh = false;
        if let (Ok(mut text), Some(report)) = (times.single_mut(), &timer.report) {
            text.0 = match report.previous_best {
                Some(before) => format!(
                    "{}   {}",
                    format_time(report.time),
                    signed(report.time - before)
                ),
                None => format_time(report.time),
            };
        }
    }
    let still = settings.as_ref().is_some_and(|s| s.reduced_motion);
    let lit = showing && !still && flash_on(party.left);
    for mut colour in &mut clocks {
        colour.set_if_neq(TextColor(if lit { AMBER } else { TEXT }));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    time: Res<Time>,
    halt: Res<Halt>,
    timer: Res<LapTimer>,
    settings: Option<Res<Settings>>,
    mut card: ResMut<Card>,
    mut panels: Query<&mut Visibility, With<Panel>>,
    mut headlines: Query<(&mut Text, &mut TextColor), (With<Headline>, Without<Detail>)>,
    mut details: Query<(&mut Text, &mut TextColor), (With<Detail>, Without<Headline>)>,
    mut spans: Query<
        (&SectorSpan, &mut TextSpan, &mut TextColor),
        (Without<Headline>, Without<Detail>, Without<MedalLine>),
    >,
    mut medals: Query<
        (&mut Text, &mut TextColor),
        (With<MedalLine>, Without<Headline>, Without<Detail>),
    >,
    context: Option<(Res<crate::track::Track>, Res<crate::car::Mode>)>,
) {
    card.left = (card.left - time.delta_secs()).max(0.0);
    let showing = card.left > 0.0 && !halt.stopped() && timer.report.is_some();
    for mut visibility in &mut panels {
        visibility.set_if_neq(if showing {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    if !card.fresh {
        return;
    }
    let Some(report) = &timer.report else {
        return;
    };
    card.fresh = false;
    let palette = crate::ui::Palette::of(settings.as_deref());
    let units = settings.map_or(Units::Kmh, |s| s.units);
    if let Ok((mut text, mut colour)) = headlines.single_mut() {
        text.0 = headline(report);
        colour.0 = if !report.valid {
            RED
        } else if report.best {
            AMBER
        } else {
            TEXT
        };
    }
    if let Ok((mut text, mut colour)) = details.single_mut() {
        if report.valid {
            text.0 = speeds(report, units);
            colour.0 = AMBER_DIM;
        } else {
            text.0 = reason(report.why);
            colour.0 = RED;
        }
    }
    if let Ok((mut text, mut colour)) = medals.single_mut() {
        let targets = context
            .as_ref()
            .and_then(|(track, mode)| crate::medals::targets(track, **mode));
        match targets.filter(|_| report.valid) {
            Some(targets) => {
                text.0 = crate::medals::standing(&targets, Some(report.time));
                colour.0 = targets
                    .medal(report.time)
                    .map_or(TEXT, crate::medals::Medal::colour);
            }
            None => text.0.clear(),
        }
    }
    for (span, mut text, mut colour) in &mut spans {
        match report.sectors.get(span.0) {
            Some(seconds) => {
                text.0 = format!("S{} {seconds:.2}   ", span.0 + 1);
                colour.0 = report
                    .splits
                    .get(span.0)
                    .map_or(TEXT, |split| split_colour(*split, palette));
            }
            None => text.0.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> LapReport {
        LapReport {
            number: 7,
            time: 73.6,
            valid: true,
            best: true,
            previous_best: Some(74.02),
            sectors: vec![11.02, 12.4],
            splits: vec![Split::Purple, Split::Green],
            why: None,
            assisted: false,
            top_speed: 21.4,
            slowest: Some((7.8, 4)),
        }
    }

    #[test]
    fn the_headline_says_what_the_lap_meant() {
        assert_eq!(headline(&report()), "LAP 7    1:13.60    NEW BEST  −0.42");
        let slower = LapReport {
            best: false,
            time: 74.2,
            ..report()
        };
        assert_eq!(headline(&slower), "LAP 7    1:14.20    +0.18 to best");
        let first = LapReport {
            previous_best: None,
            ..report()
        };
        assert_eq!(headline(&first), "LAP 7    1:13.60    FIRST TIME SET");
        let invalid = LapReport {
            valid: false,
            best: false,
            ..report()
        };
        assert_eq!(headline(&invalid), "LAP 7    1:13.60    INVALID");
    }

    #[test]
    fn only_beating_a_best_that_stood_is_celebrated() {
        assert!(celebrates(&report()));
        assert!(!celebrates(&LapReport {
            previous_best: None,
            ..report()
        }));
        assert!(!celebrates(&LapReport {
            best: false,
            ..report()
        }));
        assert!(!celebrates(&LapReport {
            valid: false,
            ..report()
        }));
    }

    #[test]
    fn the_clock_flashes_three_times_then_stays_plain() {
        let lit: Vec<bool> = (0..=24)
            .map(|tenth| flash_on(CELEBRATE_FOR - tenth as f32 * 0.1))
            .collect();
        let flashes = lit.windows(2).filter(|w| !w[0] && w[1]).count() + usize::from(lit[0]);
        assert_eq!(flashes, 3);
        assert!(lit[13..].iter().all(|on| !on));
    }

    #[test]
    fn an_invalid_lap_says_where_it_was_lost() {
        assert_eq!(
            reason(Some(Why::OffTrack { sector: 3 })),
            "All four wheels off the track in sector 3"
        );
        assert_eq!(
            reason(Some(Why::Rescued { sector: 1 })),
            "Fetched back to the road in sector 1"
        );
    }

    #[test]
    fn speeds_follow_the_chosen_units() {
        let kmh = speeds(&report(), Units::Kmh);
        assert_eq!(
            kmh,
            "Top speed 231 km/h    ·    slowest 84 km/h in sector 4"
        );
        let mph = speeds(&report(), Units::Mph);
        assert_eq!(mph, "Top speed 144 mph    ·    slowest 52 mph in sector 4");
    }
}
