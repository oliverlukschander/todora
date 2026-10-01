//! The game's opinions.
//!
//! Where the shipped game says something dry, Bonkers says something else: a
//! remark when the game is paused, a different reason for each wait while a
//! circuit loads, and on the title, while its Bonkers button is the one chosen,
//! a tagline, the logo going through the rainbow and bouncing on the beat, and a
//! sticker. The plain game gets the dry words, unchanged.

use bevy::prelude::*;

use super::Fun;
use super::beat::Beat;
use super::parts::rainbow;
use crate::pause::Halt;
use crate::text::{Tr, t, tf};

/// How many of each there are in the text table.
pub(crate) const TITLE: usize = 14;
pub(crate) const PAUSE: usize = 10;
pub(crate) const LOAD: usize = 16;

/// The `pick`th line of a pool, wrapping.
pub(crate) fn line(pool: &str, count: usize, pick: usize) -> &'static str {
    t(&format!("quip.{pool}.{}", pick % count))
}

/// The title's tagline, with the number of circuits where it wants one: shown
/// while its Bonkers button is chosen.
pub(crate) fn tagline(circuits: usize, pick: usize) -> String {
    tf(&format!("quip.title.{}", pick % TITLE), &[&circuits])
}

/// The rainbow logo on the title screen.
#[derive(Component)]
pub(crate) struct Logo;

/// The sticker next to it, that says which mode that is.
#[derive(Component)]
pub(crate) struct Sticker;

/// The sticker, and not the logo.
type StickerOnly = (With<Sticker>, Without<Logo>);

pub(super) fn plugin(app: &mut App) {
    // After the labels have been put in the new language, if that is what changed,
    // or what was said in the old one would be written over the joke.
    app.add_systems(
        Update,
        (retitle, comment.after(crate::text::retranslate), logo),
    );
}

/// The HUD's top line says which mode this is.
fn retitle(fun: Res<Fun>, mut labels: Query<(&Tr, &mut Text)>) {
    for (tr, mut text) in &mut labels {
        if tr.0 != "hud.free_drive" {
            continue;
        }
        let wanted = if fun.bonkers() {
            t("hud.edition_bonkers")
        } else {
            t("hud.free_drive")
        };
        if text.0 != wanted {
            text.0 = wanted.into();
        }
    }
}

/// The size a joke is set in where the pause has a title: it is a sentence and
/// not a word, and the panel it goes in is not wide.
const JOKE_SIZE: f32 = 22.0;

/// A remark on the way into the pause, and a reason for each wait. Put right
/// again when the game stops being Bonkers, or the language changes, while it is
/// up: whatever was written there would otherwise stay until the next.
fn comment(
    fun: Res<Fun>,
    halt: Res<Halt>,
    mut last: Local<Option<(Halt, bool, crate::text::Language)>>,
    mut labels: Query<(&Tr, &mut Text, &mut TextFont)>,
) {
    let now = (*halt, fun.bonkers(), crate::text::current());
    if last.replace(now) == Some(now) {
        return;
    }
    let (key, pool, count) = match *halt {
        Halt::Pause => ("pause.title", "pause", PAUSE),
        Halt::Loading => ("loading.wait", "load", LOAD),
        _ => return,
    };
    let pick = super::rng::Rng::random().below(count);
    for (tr, mut text, mut font) in &mut labels {
        if tr.0 != key {
            continue;
        }
        let joke = fun.bonkers();
        text.0 = if joke {
            line(pool, count, pick)
        } else {
            t(key)
        }
        .into();
        // The pause's title is large, and a joke there is set smaller.
        if key == "pause.title" {
            font.font_size = FontSize::Px(if joke {
                JOKE_SIZE
            } else {
                crate::pause::TITLE_SIZE
            });
        }
    }
}

/// The logo goes through the rainbow and bounces to the beat: in Bonkers, and
/// on the title while its Bonkers button is chosen, which is what it shows of
/// the mode before the mode is driven.
fn logo(
    fun: Res<Fun>,
    beat: Res<Beat>,
    time: Res<Time<Real>>,
    halt: Res<Halt>,
    title: Option<Res<crate::title::Title>>,
    mut logos: Query<(&mut TextColor, &mut UiTransform, &mut TextFont), With<Logo>>,
    mut stickers: Query<(&mut Node, &mut UiTransform), StickerOnly>,
) {
    let t = time.elapsed_secs();
    let party = fun.bonkers() || crate::title::Title::tempting(title.as_deref(), *halt);
    for (mut colour, mut transform, mut font) in &mut logos {
        // Written only when it is not so already: a text that is written to is a
        // text that is laid out again, and this one would be, every frame.
        let weight = if party {
            FontWeight::BLACK
        } else {
            FontWeight::NORMAL
        };
        if font.weight != weight {
            font.weight = weight;
        }
        if party {
            let kick = if fun.calm { 0.0 } else { beat.kick() };
            colour.0 = rainbow(if fun.calm { 0.1 } else { t * 0.15 }, 0.66);
            transform.scale = Vec2::splat(1.0 + 0.07 * kick);
            transform.rotation = Rot2::degrees(if fun.calm { 0.0 } else { (t * 1.7).sin() * 1.2 });
        } else {
            colour.0 = crate::ui::TEXT;
            transform.scale = Vec2::ONE;
            transform.rotation = Rot2::IDENTITY;
        }
    }
    for (mut node, mut transform) in &mut stickers {
        let display = if party { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        transform.rotation =
            Rot2::degrees(-4.0 + if fun.calm { 0.0 } else { (t * 3.1).sin() * 2.0 });
        let pop = if fun.calm {
            1.0
        } else {
            1.0 + 0.06 * beat.kick()
        };
        transform.scale = Vec2::splat(pop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_of_every_pool_is_in_the_table_and_the_counts_are_right() {
        for (pool, count) in [("title", TITLE), ("pause", PAUSE), ("load", LOAD)] {
            for pick in 0..count {
                let text = line(pool, count, pick);
                assert!(
                    text != "?" && !text.is_empty(),
                    "quip.{pool}.{pick} is missing"
                );
            }
            // And there is not one more than the count says.
            assert_eq!(
                crate::text::t_in(crate::text::Language::En, &format!("quip.{pool}.{count}")),
                "?",
                "{pool} has more lines than {count}"
            );
        }
    }

    #[test]
    fn the_pause_says_a_joke_small_and_takes_it_back_when_the_game_is_not_bonkers() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Fun>()
            .insert_resource(Halt::Pause)
            .add_systems(Update, comment);
        let title = app
            .world_mut()
            .spawn((
                Tr("pause.title"),
                Text::new(t("pause.title")),
                TextFont {
                    font_size: FontSize::Px(crate::pause::TITLE_SIZE),
                    ..default()
                },
            ))
            .id();
        let seen = |app: &App| {
            let font = app.world().get::<TextFont>(title).unwrap();
            (
                app.world().get::<Text>(title).unwrap().0.clone(),
                font.font_size,
            )
        };
        app.update();
        assert_eq!(seen(&app).0, t("pause.title"), "a serious pause is plain");
        // The game becomes Bonkers while the pause is up, as F9 does.
        app.world_mut().resource_mut::<Fun>().on = true;
        app.update();
        let (words, size) = seen(&app);
        assert_ne!(words, t("pause.title"), "no joke");
        assert_eq!(size, FontSize::Px(JOKE_SIZE));
        // And plain again: the title, at its own size.
        app.world_mut().resource_mut::<Fun>().on = false;
        app.update();
        assert_eq!(
            seen(&app),
            (
                t("pause.title").to_string(),
                FontSize::Px(crate::pause::TITLE_SIZE)
            )
        );
    }

    #[test]
    fn picks_wrap_and_taglines_fill_in_the_circuit_count() {
        assert_eq!(line("load", LOAD, 0), line("load", LOAD, LOAD));
        assert!(tagline(51, 0).contains("51"));
        assert!(!tagline(51, 2).contains("{}"));
    }
}
