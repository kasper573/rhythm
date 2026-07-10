use super::{PreferredDifficulty, Wheel, WheelEntry, slot_entry};
use crate::core::config::GameConfig;
use crate::core::font::game_font;
use crate::core::high_scores::{HighScores, highscore_key};
use crate::core::library::{StepfileId, StepfileLibrary};
use crate::core::player::PlayerId;
use crate::core::screen::at;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use strum::IntoEnumIterator;

/// On-screen height of the wheel's rating images (native art is 120px).
const RATING_HEIGHT: f32 = 38.0;
/// Right edge of the rating column in row-local coordinates; the ratings
/// are children of their wheel row, so they ride it at this inset from its
/// center as it scrolls and bulges.
const RATING_RIGHT_X: f32 = 270.0;
/// Width reserved per player's rating in versus, where both are inline:
/// wide enough for the widest rating art at [`RATING_HEIGHT`].
const RATING_SLOT_WIDTH: f32 = 92.0;

/// One player's high-score rating image at the right edge of a stepfile
/// row; versus shows both players' inline. Public only because the
/// template derive demands it; the fields stay this module's.
#[derive(Component, Clone, FromTemplate)]
pub struct SlotRating {
    slot: usize,
    player: PlayerId,
}

/// The P1/P2 tag in the bottom-left corner of a rating image, positioned
/// by [`position_rating_labels`] once the image's width is known.
#[derive(Component, Default, Clone)]
pub(super) struct RatingLabel;

/// The rating widgets of one wheel slot, one per player slot; the systems
/// here fill, place, and show them.
pub(super) fn slot_ratings(slot: usize) -> Vec<impl Scene + use<>> {
    PlayerId::iter()
        .map(|player| {
            let tag = player.label().to_string();
            bsn! {
                SlotRating { slot: slot, player: {player} }
                Sprite
                Anchor({Anchor::CENTER_RIGHT.0})
                Visibility::Hidden
                Transform {
                    translation: {Vec3::new(RATING_RIGHT_X, 0.0, 0.1)},
                    scale: {Vec3::splat(RATING_HEIGHT / 120.0)},
                }
                Children [(
                    RatingLabel
                    game_font(34.0)
                    Text2d({tag})
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.9))
                    Anchor({Anchor::BOTTOM_LEFT.0})
                    at(-120.0, -60.0, 0.1)
                )]
            }
        })
        .collect()
}

/// Packs each player's rating into its row's rating column: in versus both
/// players' ratings sit inline, the last active player's at the right edge
/// and earlier players stacked to its left.
pub(super) fn pack_player_ratings(
    wheel: Res<Wheel>,
    mut ratings: Query<(&SlotRating, &mut Transform)>,
) {
    for (rating, mut transform) in &mut ratings {
        let position = wheel
            .players
            .iter()
            .position(|player| *player == rating.player)
            .unwrap_or(0);
        let packed = (wheel.players.len() - 1 - position) as f32 * RATING_SLOT_WIDTH;
        let x = RATING_RIGHT_X - packed;
        if transform.translation.x != x {
            transform.translation.x = x;
        }
    }
}

/// Pins each rating's P1/P2 tag to the image's bottom-left corner; the
/// rating art varies in width, so the corner is only known once the image
/// has loaded.
pub(super) fn position_rating_labels(
    images: Res<Assets<Image>>,
    ratings: Query<(&Sprite, &Children), With<SlotRating>>,
    mut labels: Query<&mut Transform, With<RatingLabel>>,
) {
    for (sprite, children) in &ratings {
        let Some(image) = images.get(&sprite.image) else {
            continue;
        };
        // Child coordinates are in the image's native pixels: the sprite is
        // anchored at its right edge and scaled uniformly.
        let x = -(image.size().x as f32) + 4.0;
        for child in children.iter() {
            if let Ok(mut transform) = labels.get_mut(child)
                && transform.translation.x != x
            {
                transform.translation.x = x;
            }
        }
    }
}

pub(super) fn refresh_wheel_ratings(
    wheel: Res<Wheel>,
    library: Res<StepfileLibrary>,
    high_scores: Res<HighScores>,
    config: Res<GameConfig>,
    preferred: Res<PreferredDifficulty>,
    asset_server: Res<AssetServer>,
    mut ratings: Query<(&SlotRating, &mut Sprite, &mut Visibility)>,
) {
    if !wheel.dirty {
        return;
    }
    for (rating, mut sprite, mut visibility) in &mut ratings {
        let shown = match slot_entry(&wheel, rating.slot) {
            Some(WheelEntry::Stepfile { id }) if wheel.players.contains(&rating.player) => {
                high_score_rating(
                    *id,
                    rating.player,
                    &wheel,
                    &library,
                    &high_scores,
                    &config,
                    &preferred,
                )
                .map(|image| {
                    let image = asset_server.load(image);
                    if sprite.image != image {
                        sprite.image = image;
                    }
                })
            }
            _ => None,
        };
        visibility.set_if_neq(match shown {
            Some(()) => Visibility::Visible,
            None => Visibility::Hidden,
        });
    }
}

/// The rating image earned by the player's high score on the chart this
/// row would currently play them. Only the total points are stored, so
/// grade-based rating rules never match here.
fn high_score_rating(
    id: StepfileId,
    player: PlayerId,
    wheel: &Wheel,
    library: &StepfileLibrary,
    high_scores: &HighScores,
    config: &GameConfig,
    preferred: &PreferredDifficulty,
) -> Option<String> {
    let entry = library.stepfile(id);
    let chart = &entry.stepfile.charts[wheel.chart_for(&entry.stepfile, preferred, player)?];
    let key = highscore_key(library, id, chart);
    let points = high_scores.get(player, &key)?;
    let stats = chart.stats();
    let percent = config.score_percent(points, chart.rows.len() as u32, stats.holds as u32);
    Some(config.rating(percent, None).image.clone())
}
