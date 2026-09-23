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
const PURPLE: Color = Color::srgb(0.74, 0.42, 1.0);
const GREEN: Color = Color::srgb(0.38, 0.86, 0.42);
const YELLOW: Color = Color::srgb(0.98, 0.83, 0.27);
const RED: Color = Color::srgb(0.96, 0.32, 0.26);

pub struct SummaryPlugin;

impl Plugin for SummaryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Card>()
            .add_systems(Startup, setup)
            .add_systems(FixedUpdate, hear.after(LapSet))
            .add_systems(Update, draw);
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
#[derive(Component)]
struct SectorSpan(usize);

/// The first line: which lap, its time, and what it meant.
pub(crate) fn headline(report: &LapReport) -> String {
    let time = format_time(report.time);
    let meaning = if !report.valid {
        "INVALID".to_string()
    } else if report.best {
        match report.previous_best {
            Some(before) => format!("NEW BEST  {}", signed(report.time - before)),
            None => "FIRST TIME SET".into(),
        }
    } else {
        match report.previous_best {
            Some(best) => format!("{} to best", signed(report.time - best)),
            None => String::new(),
        }
    };
    format!("LAP {}    {time}    {meaning}", report.number)
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
    match why {
        Some(Why::OffTrack { sector }) => {
            format!("All four wheels off the track in sector {sector}")
        }
        Some(Why::Rescued { sector }) => format!("Fetched back to the road in sector {sector}"),
        Some(Why::Paused) => "Paused during shared practice".into(),
        None => "Did not count".into(),
    }
}

/// The last line: how fast and how slow, in the chosen units.
pub(crate) fn speeds(report: &LapReport, units: Units) -> String {
    let unit = match units {
        Units::Kmh => "km/h",
        Units::Mph => "mph",
    };
    let shown = |speed| crate::hud::displayed_speed(speed, units);
    let top = format!("Top speed {:.0} {unit}", shown(report.top_speed));
    match report.slowest {
        Some((speed, sector)) => format!(
            "{top}    ·    slowest {:.0} {unit} in sector {sector}",
            shown(speed)
        ),
        None => top,
    }
}

fn split_colour(split: Split) -> Color {
    match split {
        Split::Purple => PURPLE,
        Split::Green => GREEN,
        Split::Yellow => YELLOW,
        Split::Plain => TEXT,
    }
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
            });
        });
}

/// A lap has ended: put the card up.
fn hear(mut laps: MessageReader<LapFinished>, mut card: ResMut<Card>) {
    if laps.read().last().is_some() {
        card.left = SHOWN_FOR;
        card.fresh = true;
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
        (Without<Headline>, Without<Detail>),
    >,
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
    for (span, mut text, mut colour) in &mut spans {
        match report.sectors.get(span.0) {
            Some(seconds) => {
                text.0 = format!("S{} {seconds:.2}   ", span.0 + 1);
                colour.0 = report
                    .splits
                    .get(span.0)
                    .map_or(TEXT, |split| split_colour(*split));
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
