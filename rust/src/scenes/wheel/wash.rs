use super::{WHEEL_EASE_RATE, WheelEntry, WheelScene};
use crate::core::library::library;
use crate::core::screen::linear_blend;
use crate::core::units::Seconds;
use crate::nodes::media_cover::{MediaCover, MediaCoverOptions, MediaPace};
use godot::prelude::*;
use std::path::PathBuf;

const BACKGROUND_OPACITY: f32 = 0.25;

/// The scene background wash: the active row's background — image or
/// looping video — over the green backdrop, as stacked media covers.
/// Changing rows cross-fades: the incoming layer waits invisible until its
/// image has actually loaded, then retires every older layer while it
/// eases in — so the old background always fades under a renderable new
/// one, never against a gap that a late-loading image would pop into.
#[derive(Default)]
pub(super) struct Wash {
    layers: Vec<WashLayer>,
    sequence: u32,
}

struct WashLayer {
    cover: Gd<MediaCover>,
    /// The opacity this layer eases toward; reaching zero retires it.
    target: f32,
    /// Spawn order; the newest layer leads and retires the older ones.
    sequence: u32,
    /// The file this layer shows, the identity that keeps a re-selected
    /// background from restarting.
    source: PathBuf,
}

impl WheelScene {
    pub(super) fn refresh_wash(&mut self) {
        if !self.just_settled {
            return;
        }
        // Rows without a background of their own fall back to the default
        // BGM's, so the scene always has one to show.
        let path = match self.entries.get(self.active) {
            Some(WheelEntry::Stepfile { id }) => library().stepfile(*id).background_path(),
            _ => None,
        }
        .or_else(|| library().default_bgm.background_path());
        let Some(path) = path else {
            // Nothing to show at all: fade everything out.
            for layer in &mut self.wash.layers {
                layer.target = 0.0;
            }
            return;
        };
        let already_shown = self
            .wash
            .layers
            .iter()
            .any(|layer| layer.target > 0.0 && layer.source == path);
        if already_shown {
            return;
        }
        let cover = MediaCover::instantiate(MediaCoverOptions {
            path: path.clone(),
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.0),
            z: 5,
            start: Seconds::ZERO,
            looping: true,
            pace: MediaPace::Wall,
        });
        let Some(cover) = cover else { return };
        // Later siblings draw above the layers fading out beneath them.
        self.base_mut().add_child(&cover);
        self.wash.sequence += 1;
        let sequence = self.wash.sequence;
        self.wash.layers.push(WashLayer {
            cover,
            target: linear_blend(BACKGROUND_OPACITY),
            sequence,
            source: path,
        });
    }

    /// Eases every wash layer toward its target opacity at the wheel's
    /// settle rate and retires the fully faded-out ones. Layers whose image
    /// is still loading hold at zero: only a loaded layer may lead, and
    /// only the leader retires the layers beneath it.
    pub(super) fn fade_wash(&mut self, delta: f64) {
        let leader = self
            .wash
            .layers
            .iter()
            .filter(|layer| layer.target > 0.0 && layer.cover.bind().is_ready())
            .map(|layer| layer.sequence)
            .max();
        if let Some(leader_sequence) = leader {
            for layer in &mut self.wash.layers {
                if layer.sequence < leader_sequence && layer.target > 0.0 {
                    layer.target = 0.0;
                }
            }
        }

        let ease = 1.0 - (-WHEEL_EASE_RATE * delta as f32).exp();
        self.wash.layers.retain_mut(|layer| {
            if layer.target > 0.0 && !layer.cover.bind().is_ready() {
                return true;
            }
            let mut modulate = layer.cover.get_modulate();
            let mut next = modulate.a + (layer.target - modulate.a) * ease;
            if (next - layer.target).abs() < 0.002 {
                next = layer.target;
            }
            if next != modulate.a {
                modulate.a = next;
                layer.cover.set_modulate(modulate);
            }
            if layer.target <= 0.0 && next <= 0.0 {
                layer.cover.queue_free();
                return false;
            }
            true
        });
    }
}
