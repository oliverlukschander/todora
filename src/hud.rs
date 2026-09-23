use bevy::prelude::*;

use crate::car::{Car, Mode, Setup, Spec};
use crate::ghost::Ghost;
use crate::lap::{LapTimer, Split, format_time};
use crate::settings::Units;
use crate::track::Track;

pub(crate) const AMBER: Color = crate::ui::ACCENT;
pub(crate) const AMBER_DIM: Color = crate::ui::MUTED;
pub(crate) const PANEL: Color = Color::srgba(0.035, 0.05, 0.065, 0.9);
pub(crate) const FRONT: Color = Color::srgba(0.045, 0.06, 0.073, 0.98);
/// What the invalid-lap warning is written in.
const BEHIND: Color = crate::ui::Palette::STANDARD.behind;
/// The most sectors any circuit is split into.
const MOST_SECTORS: usize = 8;

/// Arcade speedometer calibration for the miniature world.
/// Shared by all cars and independent of model size; physics stays in m/s.
pub(crate) const DISPLAY_SPEED_SCALE: f32 = 3.0;

/// Side of the g-meter's square, in pixels, and the reading that reaches its
/// edge. Tyres give up somewhere near 1.2 g, so a needle on the rim means the
/// car is at the limit.
const METER: f32 = 92.0;
const FULL_SCALE: f32 = 1.4;
const NEEDLE: f32 = 12.0;
/// The setup slider: as wide as the meter above it, with three notches the knob
/// snaps between.
const SLIDER: f32 = METER;
const KNOB: f32 = 26.0;
const TRACK: f32 = 10.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            Update,
            (
                show,
                draw_controls,
                draw_clock,
                draw_sector,
                draw_sector_bar,
                draw_g_meter,
                draw_setup,
                draw_car,
                draw_delta,
                draw_circuit,
                draw_optional,
                draw_contrast,
                (place, keep_safe).chain(),
            )
                .after(crate::ghost::GhostSet),
        );
    }
}

#[derive(Component)]
pub(crate) struct ClockReadout;
#[derive(Component)]
struct SectorReadout;
#[derive(Component)]
struct InvalidReadout;
/// One segment of the sector bar under the clock.
#[derive(Component)]
struct SectorSegment(usize);

#[derive(Component)]
struct Needle;

#[derive(Component)]
struct GReadout;

#[derive(Component)]
struct SpeedReadout;

#[derive(Component)]
struct SetupKnob;

#[derive(Component)]
struct SetupName;

#[derive(Component)]
struct CarName;

#[derive(Component)]
struct DeltaReadout;
/// The downloaded rival's delta, when both ghosts are shown.
#[derive(Component)]
struct RivalDelta;
/// "KM/H" or "MPH" under the speed.
#[derive(Component)]
struct SpeedUnit;
/// The g-meter and its reading, which the settings can hide.
#[derive(Component)]
struct GMeterPart;

/// Where a HUD panel was placed, before any TV-safe margin moved it in.
#[derive(Component, Clone, Copy)]
struct Placed {
    top: Val,
    right: Val,
    bottom: Val,
    left: Val,
}

/// How far each edge moves in for a TV: 5% of the screen, the usual title-safe
/// allowance.
const TV_SAFE: f32 = 0.05;

#[derive(Component)]
struct CircuitName;

#[derive(Component)]
pub(crate) struct Instrument;

#[derive(Component)]
struct ControlHints;

fn setup(mut commands: Commands) {
    use crate::ui::{LINE, TEXT, label};
    commands
        .spawn((
            Instrument,
            Node {
                position_type: PositionType::Absolute,
                top: px(24),
                left: px(24),
                padding: UiRect::axes(px(18), px(14)),
                border_radius: BorderRadius::all(px(10)),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|session| {
            session.spawn(crate::text::label("hud.free_drive", 12.0, AMBER));
            session.spawn((CircuitName, label("", 23.0, TEXT)));
        });
    commands
        .spawn((
            Instrument,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(94),
                left: px(24),
                padding: UiRect::all(px(18)),
                border_radius: BorderRadius::all(px(12)),
                column_gap: px(24),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|instruments| {
            instruments
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(5),
                    min_width: px(138),
                    ..default()
                })
                .with_children(|speed| {
                    speed.spawn((CarName, label("", 12.0, AMBER_DIM)));
                    speed.spawn((SpeedReadout, label("0", 48.0, TEXT)));
                    speed.spawn((SpeedUnit, label("KM/H", 11.0, AMBER_DIM)));
                    speed.spawn((SetupName, label("", 11.0, AMBER)));
                });
            instruments
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(9),
                    ..default()
                })
                .with_children(|g| {
                    g.spawn((
                        GMeterPart,
                        Node {
                            width: px(METER),
                            height: px(METER),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(METER / 2.0)),
                            ..default()
                        },
                        BorderColor::all(LINE),
                    ))
                    .with_children(|meter| {
                        for horizontal in [true, false] {
                            meter.spawn((
                                Node {
                                    position_type: PositionType::Absolute,
                                    left: px(if horizontal { 8.0 } else { METER / 2.0 }),
                                    top: px(if horizontal { METER / 2.0 } else { 8.0 }),
                                    width: px(if horizontal { METER - 16.0 } else { 1.0 }),
                                    height: px(if horizontal { 1.0 } else { METER - 16.0 }),
                                    ..default()
                                },
                                BackgroundColor(LINE),
                            ));
                        }
                        meter.spawn((
                            Needle,
                            Node {
                                position_type: PositionType::Absolute,
                                left: px((METER - NEEDLE) / 2.0),
                                top: px((METER - NEEDLE) / 2.0),
                                width: px(NEEDLE),
                                height: px(NEEDLE),
                                border_radius: BorderRadius::all(px(NEEDLE / 2.0)),
                                ..default()
                            },
                            BackgroundColor(AMBER),
                        ));
                    });
                    g.spawn((GReadout, GMeterPart, label("0.00 G", 12.0, AMBER_DIM)));
                    g.spawn((
                        Node {
                            width: px(SLIDER),
                            height: px(TRACK),
                            border_radius: BorderRadius::all(px(5)),
                            ..default()
                        },
                        BackgroundColor(LINE),
                    ))
                    .with_children(|slider| {
                        for i in 0..3 {
                            slider.spawn(notch_mark(i));
                        }
                        slider.spawn((
                            SetupKnob,
                            Node {
                                position_type: PositionType::Absolute,
                                width: px(KNOB),
                                height: px(TRACK),
                                border_radius: BorderRadius::all(px(5)),
                                ..default()
                            },
                            BackgroundColor(AMBER),
                        ));
                    });
                });
        });
    commands
        .spawn((
            Instrument,
            Node {
                position_type: PositionType::Absolute,
                top: px(240),
                right: px(24),
                padding: UiRect::all(px(20)),
                border_radius: BorderRadius::all(px(12)),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                min_width: px(220),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|timing| {
            timing.spawn(crate::text::label("hud.timing", 11.0, AMBER_DIM));
            timing.spawn((
                ClockReadout,
                label(clock_text(&LapTimer::default()), 24.0, TEXT),
            ));
            timing
                .spawn(Node {
                    column_gap: px(4),
                    ..default()
                })
                .with_children(|bar| {
                    for i in 0..MOST_SECTORS {
                        bar.spawn((
                            SectorSegment(i),
                            Node {
                                flex_grow: 1.0,
                                height: px(8),
                                border_radius: BorderRadius::all(px(4)),
                                ..default()
                            },
                            BackgroundColor(crate::ui::LINE),
                        ));
                    }
                });
            timing.spawn((InvalidReadout, label("", 18.0, BEHIND)));
            timing.spawn((SectorReadout, label("", 17.0, AMBER_DIM)));
            timing.spawn((DeltaReadout, label("GHOST  --", 20.0, AMBER_DIM)));
            timing.spawn((RivalDelta, label("", 18.0, AMBER_DIM)));
        });
    commands.spawn((Instrument, ControlHints, label("WASD / Arrows  Drive    Space  Handbrake    R  Restart\nG  Ghost    Scroll  Zoom    Esc  Pause", 12.0, TEXT), Node {
        position_type: PositionType::Absolute, bottom: px(24), left: px(24),
        padding: UiRect::axes(px(14), px(10)), border_radius: BorderRadius::all(px(8)), ..default()
    }, BackgroundColor(PANEL)));
}

fn draw_controls(
    pads: Query<&Gamepad>,
    sound: Res<crate::sound::Sound>,
    mut text: Query<&mut Text, With<ControlHints>>,
) {
    use crate::text::{t, tf};
    let keys = t(if pads.is_empty() {
        "hud.keys"
    } else {
        "hud.pad"
    });
    let effects = t(if sound.effects { "word.on" } else { "word.off" }).to_lowercase();
    let hint = format!("{keys}\n{}", tf("hud.sound", &[&sound.station(), &effects]));
    if let Ok(mut text) = text.single_mut()
        && text.0 != hint
    {
        text.0 = hint;
    }
}

fn show(halt: Res<crate::pause::Halt>, mut instruments: Query<&mut Visibility, With<Instrument>>) {
    if halt.is_changed() {
        for mut visible in &mut instruments {
            *visible = if halt.stopped() {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
    }
}

/// A tick on the slider's track, centred under the notch the knob sits on.
fn notch_mark(notch: usize) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: px(knob_left(notch) + KNOB / 2.0 - 1.0),
            top: px(TRACK / 2.0 - 1.0),
            width: px(2),
            height: px(2),
            ..default()
        },
        BackgroundColor(AMBER_DIM),
    )
}

/// Where the knob's left edge sits for a notch.
fn knob_left(notch: usize) -> f32 {
    notch as f32 * (SLIDER - KNOB) / 2.0
}

/// Slide the knob to the chosen notch and name it.
fn draw_setup(
    chosen: Res<Setup>,
    mut knob: Query<&mut Node, With<SetupKnob>>,
    mut name: Query<&mut Text, With<SetupName>>,
) {
    if !chosen.is_changed() {
        return;
    }
    if let Ok(mut node) = knob.single_mut() {
        node.left = px(knob_left(chosen.notch()));
    }
    if let Ok(mut text) = name.single_mut() {
        text.0 = chosen.name().into();
    }
}

/// Name the car being driven. It sits over the meter rather than in the menu,
/// because which car you are in is a thing you want to know while driving it and
/// the menu is only up when you are not.
fn draw_car(spec: Res<Spec>, mode: Res<Mode>, mut readout: Query<&mut Text, With<CarName>>) {
    if !spec.is_changed() && !mode.is_changed() {
        return;
    }
    if let Ok(mut text) = readout.single_mut() {
        text.0 = format!("{} / {}", spec.name(), mode.name().to_uppercase());
    }
}

/// Name the circuit being driven, so the board above it is read against the
/// right one.
fn draw_circuit(track: Res<Track>, mut readout: Query<&mut Text, With<CircuitName>>) {
    if !track.is_changed() {
        return;
    }
    if let Ok(mut text) = readout.single_mut() {
        text.0 = track.circuit().name.to_uppercase();
    }
}

/// The gap to the ghost, keyed to where the car is on the circuit: how many
/// seconds ahead or behind this lap is against the best, right now.
/// The gap to whichever ghost is shown: yours first, the downloaded one
/// below it when both are, or the downloaded one alone.
#[allow(clippy::type_complexity)]
fn draw_delta(
    ghost: Res<Ghost>,
    rival: Option<Res<crate::ghost::Rival>>,
    settings: Option<Res<crate::settings::Settings>>,
    mut readout: Query<(&mut Text, &mut TextColor), (With<DeltaReadout>, Without<RivalDelta>)>,
    mut second: Query<(&mut Text, &mut TextColor), (With<RivalDelta>, Without<DeltaReadout>)>,
) {
    let palette = crate::ui::Palette::of(settings.as_deref());
    let (ahead, behind) = (palette.ahead, palette.behind);
    let theirs = rival
        .as_ref()
        .filter(|r| r.on && r.loaded())
        .map(|r| (rival_label(r), r.delta));
    let gap = |who: &str, delta: Option<f32>| match delta {
        None => (format!("{who}  --"), AMBER_DIM),
        Some(delta) => (
            format!("{who}  {delta:+.2}"),
            if delta <= 0.0 { ahead } else { behind },
        ),
    };
    let (first, below) = match (ghost.on, &theirs) {
        // Your ghost alone reads as it always has: just the gap.
        (true, None) => match ghost.delta {
            Some(delta) => (
                (
                    format!("{delta:+.2}"),
                    if delta <= 0.0 { ahead } else { behind },
                ),
                None,
            ),
            None => (
                (format!("{}  --", crate::text::t("hud.ghost")), AMBER_DIM),
                None,
            ),
        },
        (true, Some((who, delta))) => (gap("PB", ghost.delta), Some(gap(who, *delta))),
        (false, Some((who, delta))) => (gap(who, *delta), None),
        (false, None) => (
            (crate::text::t("hud.ghost_off").to_string(), AMBER_DIM),
            None,
        ),
    };
    if let Ok((mut text, mut color)) = readout.single_mut() {
        if text.0 != first.0 {
            text.0 = first.0;
        }
        color.0 = first.1;
    }
    if let Ok((mut text, mut color)) = second.single_mut() {
        let (line, tint) = below.unwrap_or((String::new(), AMBER_DIM));
        if text.0 != line {
            text.0 = line;
        }
        color.0 = tint;
    }
}

/// "WR" for the world record, else the driver's name, kept short.
fn rival_label(rival: &crate::ghost::Rival) -> String {
    if rival.rank == Some(1) {
        "WR".into()
    } else if rival.name.is_empty() {
        "GHOST".into()
    } else {
        rival.name.chars().take(10).collect()
    }
}

fn draw_clock(timer: Res<LapTimer>, mut readout: Query<&mut Text, With<ClockReadout>>) {
    if !timer.is_changed() {
        return;
    }
    let Ok(mut text) = readout.single_mut() else {
        return;
    };
    text.0 = clock_text(&timer);
}

/// Put the needle where the car is pulling. Cornering moves it sideways,
/// braking and the climb out of a dip move it up and down.
fn draw_g_meter(
    settings: Option<Res<crate::settings::Settings>>,
    cars: Query<&Car>,
    mut needle: Query<&mut Node, With<Needle>>,
    mut readout: Query<&mut Text, With<GReadout>>,
    mut speed: Query<&mut Text, (With<SpeedReadout>, Without<GReadout>)>,
) {
    let Ok(car) = cars.single() else {
        return;
    };
    if let Ok(mut text) = speed.single_mut() {
        // A hill adds a lot of speed, and a corner that will not come round is
        // usually a corner arrived at too fast. Worth being able to see.
        let units = settings.as_ref().map_or(Units::Kmh, |s| s.units);
        text.0 = format!("{:.0}", displayed_speed(car.velocity.length(), units));
    }
    let reading = (car.g_force / FULL_SCALE).clamp_length_max(1.0) * (METER - NEEDLE) / 2.0;
    if let Ok(mut node) = needle.single_mut() {
        node.left = px((METER - NEEDLE) / 2.0 + reading.x);
        // Screen y grows downward, so accelerating pushes the needle up.
        node.top = px((METER - NEEDLE) / 2.0 - reading.y);
    }
    if let Ok(mut text) = readout.single_mut() {
        text.0 = format!("{:.2} G", car.g_force.length());
    }
}

/// The speedometer's reading for `speed` metres a second, in the chosen units.
pub(crate) fn displayed_speed(speed: f32, units: Units) -> f32 {
    let kmh = speed * 3.6 * DISPLAY_SPEED_SCALE;
    match units {
        Units::Kmh => kmh,
        Units::Mph => kmh / 1.609_344,
    }
}

/// The unit label and the g-meter, as the settings say.
fn draw_optional(
    settings: Option<Res<crate::settings::Settings>>,
    mut units: Query<&mut Text, With<SpeedUnit>>,
    mut parts: Query<&mut Node, With<GMeterPart>>,
) {
    let Some(settings) = settings.filter(|s| s.is_changed()) else {
        return;
    };
    for mut text in &mut units {
        text.0 = match settings.units {
            Units::Kmh => "KM/H",
            Units::Mph => "MPH",
        }
        .into();
    }
    for mut node in &mut parts {
        node.display = if settings.g_meter {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// Opaque black panels for the high-contrast HUD, or the usual smoke.
fn draw_contrast(
    settings: Option<Res<crate::settings::Settings>>,
    mut panels: Query<&mut BackgroundColor, With<Instrument>>,
    added: Query<(), Added<Instrument>>,
) {
    let changed = settings.as_ref().is_some_and(|s| s.is_changed());
    if !changed && added.is_empty() {
        return;
    }
    let wanted = if settings.is_some_and(|s| s.high_contrast) {
        Color::BLACK
    } else {
        PANEL
    };
    for mut colour in &mut panels {
        colour.set_if_neq(BackgroundColor(wanted));
    }
}

/// Note where every HUD panel was put, the first time it is seen.
#[allow(clippy::type_complexity)]
fn place(
    mut commands: Commands,
    panels: Query<(Entity, &Node), (With<Instrument>, Without<Placed>)>,
) {
    for (panel, node) in &panels {
        commands.entity(panel).insert(Placed {
            top: node.top,
            right: node.right,
            bottom: node.bottom,
            left: node.left,
        });
    }
}

/// Move every panel in from the edges by the TV-safe margin, or back out.
fn keep_safe(
    settings: Option<Res<crate::settings::Settings>>,
    windows: Query<&Window>,
    scale: Res<UiScale>,
    mut panels: Query<(&Placed, &mut Node), With<Instrument>>,
    added: Query<(), Added<Placed>>,
) {
    let on = settings.as_ref().is_some_and(|s| s.tv_margin);
    let moved = settings.as_ref().is_some_and(|s| s.is_changed()) || scale.is_changed();
    if !moved && added.is_empty() {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let (across, down) = if on {
        safe_margin(window.width(), window.height(), scale.0)
    } else {
        (0.0, 0.0)
    };
    for (placed, mut node) in &mut panels {
        node.top = inset(placed.top, down);
        node.bottom = inset(placed.bottom, down);
        node.left = inset(placed.left, across);
        node.right = inset(placed.right, across);
    }
}

/// The TV-safe margin across and down, in UI pixels.
fn safe_margin(width: f32, height: f32, scale: f32) -> (f32, f32) {
    (width * TV_SAFE / scale, height * TV_SAFE / scale)
}

/// An edge moved in by `by`, if it is pinned in pixels at all.
fn inset(edge: Val, by: f32) -> Val {
    match edge {
        Val::Px(at) => Val::Px(at + by),
        other => other,
    }
}

fn clock_text(timer: &LapTimer) -> String {
    let last = timer
        .last
        .map(format_time)
        .unwrap_or_else(|| "--:--.--".into());
    let best = timer
        .best
        .map(format_time)
        .unwrap_or_else(|| "--:--.--".into());
    crate::text::tf(
        "hud.clock",
        &[&timer.completed, &format_time(timer.current), &last, &best],
    )
}

/// What colour a sector went, or `None` for one with nothing to go by.
pub(crate) fn split_colour(split: Split, palette: crate::ui::Palette) -> Option<Color> {
    match split {
        Split::Purple => Some(palette.purple),
        Split::Green => Some(palette.green),
        Split::Yellow => Some(palette.yellow),
        Split::Plain => None,
    }
}

/// A word for how a sector went, for the colour-blind palette: colour is never
/// the only thing that says it.
pub(crate) fn split_word(split: Split) -> &'static str {
    match split {
        Split::Purple => crate::text::t("hud.fastest"),
        Split::Green => crate::text::t("hud.faster"),
        Split::Yellow => crate::text::t("hud.slower"),
        Split::Plain => "",
    }
}

/// One segment per sector of this circuit, coloured as each is driven.
fn draw_sector_bar(
    timer: Res<LapTimer>,
    track: Res<Track>,
    settings: Option<Res<crate::settings::Settings>>,
    mut segments: Query<(&SectorSegment, &mut Node, &mut BackgroundColor)>,
) {
    let repainted = settings.as_ref().is_some_and(|s| s.is_changed());
    if !timer.is_changed() && !track.is_changed() && !repainted {
        return;
    }
    let palette = crate::ui::Palette::of(settings.as_deref());
    let count = track.sector_count();
    for (segment, mut node, mut colour) in &mut segments {
        let display = if segment.0 < count {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        let wanted = match timer.splits.get(segment.0) {
            Some(split) => split_colour(*split, palette).unwrap_or(crate::ui::TEXT),
            None => crate::ui::LINE,
        };
        colour.set_if_neq(BackgroundColor(wanted));
    }
}

fn draw_sector(
    timer: Res<LapTimer>,
    settings: Option<Res<crate::settings::Settings>>,
    mut sectors: Query<(&mut Text, &mut TextColor), With<SectorReadout>>,
    mut invalid: Query<&mut Text, (With<InvalidReadout>, Without<SectorReadout>)>,
) {
    if let Ok(mut text) = invalid.single_mut() {
        let wanted = if timer.invalid {
            crate::text::t("hud.invalid")
        } else {
            ""
        };
        if text.0 != wanted {
            text.0 = wanted.into();
        }
    }
    if let Ok((mut text, mut color)) = sectors.single_mut() {
        let palette = crate::ui::Palette::of(settings.as_deref());
        let words = settings.as_ref().is_some_and(|s| s.colour_blind);
        match timer.sector_notice {
            Some(s) => {
                let word = if words { split_word(s.split) } else { "" };
                text.0 = match s.delta {
                    Some(d) => {
                        crate::text::tf("hud.sector", &[&s.number, &format!("{d:+.2}"), &word])
                    }
                    None => crate::text::tf("hud.sector", &[&s.number, &format_time(s.time), &""]),
                };
                color.0 = split_colour(s.split, palette).unwrap_or(AMBER_DIM);
            }
            None => text.0.clear(),
        }
    }
}
