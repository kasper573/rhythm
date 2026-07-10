use super::{WheelEntry, WheelScene};
use crate::core::assets::asset_root;
use crate::core::config::config;
use crate::core::font::label;
use crate::core::high_scores::{HighScores, highscore_key};
use crate::core::library::{StepfileId, library};
use crate::core::player::PlayerId;
use crate::core::textures::PendingTexture;
use godot::classes::{Label, Node2D, Sprite2D};
use godot::prelude::*;
use std::path::PathBuf;
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
/// row; versus shows both players' inline, with a P1/P2 tag pinned to the
/// image's bottom-left corner once its width is known.
pub(super) struct RatingUi {
    player: PlayerId,
    sprite: Gd<Sprite2D>,
    tag: Gd<Label>,
    /// The loaded art's native width, for the right-edge anchor and the
    /// tag; `None` until the texture arrives.
    width: Option<f32>,
    source: Option<PathBuf>,
    pending: Option<PendingTexture>,
}

impl RatingUi {
    pub fn spawn(row: &mut Gd<Node2D>, player: PlayerId) -> RatingUi {
        let mut sprite = Sprite2D::new_alloc();
        sprite.set_centered(false);
        sprite.set_scale(Vector2::splat(RATING_HEIGHT / 120.0));
        sprite.set_position(Vector2::new(RATING_RIGHT_X, 0.0));
        sprite.set_visible(false);
        sprite.set_z_index(1);
        let mut tag = label(player.label(), 34.0, Color::from_rgba(1.0, 1.0, 1.0, 0.9));
        sprite.add_child(&tag);
        tag.set_position(Vector2::new(4.0, 120.0 - 34.0));
        row.add_child(&sprite);
        RatingUi {
            player,
            sprite,
            tag,
            width: None,
            source: None,
            pending: None,
        }
    }
}

impl WheelScene {
    /// Packs each player's rating into its row's rating column: in versus
    /// both players' ratings sit inline, the last active player's at the
    /// right edge and earlier players stacked to its left.
    pub(super) fn pack_player_ratings(&mut self) {
        for slot in &mut self.slots {
            for player in PlayerId::iter() {
                let rating = &mut slot.ratings[player];
                let position = self
                    .players
                    .iter()
                    .position(|active| *active == rating.player)
                    .unwrap_or(0);
                let packed =
                    (self.players.len().saturating_sub(1 + position)) as f32 * RATING_SLOT_WIDTH;
                let right_x = RATING_RIGHT_X - packed;
                // Anchored by the image's right edge and vertical center.
                let width = rating.width.unwrap_or(120.0);
                let scale = RATING_HEIGHT / 120.0;
                rating
                    .sprite
                    .set_position(Vector2::new(right_x - width * scale, -RATING_HEIGHT / 2.0));
            }
        }
    }

    /// Pins each rating's P1/P2 tag to the image's bottom-left corner and
    /// resolves textures that finished loading; the rating art varies in
    /// width, so both wait for the image.
    pub(super) fn position_rating_labels(&mut self) {
        for slot in &mut self.slots {
            for player in PlayerId::iter() {
                let rating = &mut slot.ratings[player];
                if let Some(pending) = &mut rating.pending
                    && let Some(result) = pending.poll()
                {
                    rating.pending = None;
                    if let Some(texture) = result {
                        rating.width = Some(texture.get_width() as f32);
                        rating.sprite.set_texture(&texture);
                        rating.sprite.set_visible(true);
                        let height = texture.get_height() as f32;
                        rating.tag.set_position(Vector2::new(4.0, height - 40.0));
                    }
                }
            }
        }
    }

    pub(super) fn refresh_wheel_ratings(&mut self) {
        if !self.dirty {
            return;
        }
        for index in 0..self.slots.len() {
            let entry = self.slot_entry(index);
            for player in PlayerId::iter() {
                let wanted = match entry {
                    Some(WheelEntry::Stepfile { id }) if self.players.contains(&player) => {
                        self.high_score_rating(id, player)
                    }
                    _ => None,
                };
                let rating = &mut self.slots[index].ratings[player];
                if wanted == rating.source {
                    if wanted.is_none() {
                        rating.sprite.set_visible(false);
                    }
                    continue;
                }
                rating.source = wanted.clone();
                match wanted {
                    Some(path) => {
                        rating.pending = Some(PendingTexture::load(path));
                    }
                    None => {
                        rating.pending = None;
                        rating.sprite.set_visible(false);
                    }
                }
            }
        }
    }

    /// The rating image earned by the player's high score on the chart this
    /// row would currently play them. Only the total points are stored, so
    /// grade-based rating rules never match here.
    fn high_score_rating(&self, id: StepfileId, player: PlayerId) -> Option<PathBuf> {
        let entry = library().stepfile(id);
        let chart = &entry.stepfile.charts[self.chart_for(&entry.stepfile, player)?];
        let key = highscore_key(library(), id, chart);
        let points = HighScores::singleton().bind().get(player, &key)?;
        let stats = chart.stats();
        let percent = config().score_percent(points, chart.rows.len() as u32, stats.holds as u32);
        Some(asset_root().join(&config().rating(percent, None).image))
    }
}
