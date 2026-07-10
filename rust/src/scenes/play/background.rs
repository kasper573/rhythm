use crate::core::library::StepfileEntry;
use crate::core::stepfile::StepfileTiming;
use crate::core::units::Seconds;
use crate::nodes::media_cover::{MediaCover, MediaCoverOptions, MediaPace};
use godot::classes::Control;
use godot::prelude::*;
use std::path::PathBuf;

/// How long a `CrossFade` transition blends between backgrounds.
const CROSSFADE_SECONDS: f32 = 0.5;

/// Dimmed so arrows and text stay readable in front of the background.
const DIM: f32 = 0.5;

/// The play stage's backgrounds: the stepfile's `#BGCHANGES` timeline of
/// media covers, cued on the musical timeline, cross-faded, and paced by
/// the session's visible clock so videos stay locked to the music. Layers
/// live in their own host under everything else the scene draws; newer
/// layers are later siblings, drawing above the ones fading out.
pub(super) struct Backgrounds {
    host: Gd<Control>,
    /// The stepfile's own background, shown before any timed change.
    initial: Option<PathBuf>,
    changes: Vec<BackgroundChange>,
    next: usize,
    layers: Vec<Layer>,
}

struct BackgroundChange {
    time: Seconds,
    path: PathBuf,
    crossfade: bool,
    loops: bool,
}

/// One background cover on screen, easing toward its target opacity;
/// fully faded-out layers retire.
struct Layer {
    cover: Gd<MediaCover>,
    target: f32,
}

impl Backgrounds {
    pub fn new(scene: &mut Control, entry: &StepfileEntry, timing: &StepfileTiming) -> Backgrounds {
        let mut host = Control::new_alloc();
        host.set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);
        host.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
        scene.add_child(&host);
        let mut changes: Vec<BackgroundChange> = entry
            .stepfile
            .bg_changes
            .iter()
            .filter_map(|change| {
                let path = entry.resolve_file(&change.file)?;
                Some(BackgroundChange {
                    time: timing.seconds_at_beat(change.beat),
                    path,
                    crossfade: change.crossfade,
                    loops: change.loops,
                })
            })
            .collect();
        changes.sort_by(|a, b| a.time.0.total_cmp(&b.time.0));
        Backgrounds {
            host,
            initial: entry.background_path(),
            changes,
            next: 0,
            layers: Vec::new(),
        }
    }

    /// Cues due background changes, runs the cross-fades, and locks every
    /// video to the session's visible timeline.
    pub fn update(&mut self, visible: Seconds, delta: f32) {
        if let Some(path) = self.initial.take() {
            self.apply(Seconds::ZERO, path, false, false);
        }
        while self.next < self.changes.len() && self.changes[self.next].time.0 <= visible.0 {
            let change = &self.changes[self.next];
            let (time, path, crossfade, loops) = (
                change.time,
                change.path.clone(),
                change.crossfade,
                change.loops,
            );
            self.next += 1;
            self.apply(time, path, crossfade, loops);
        }

        let step = delta / CROSSFADE_SECONDS;
        self.layers.retain_mut(|layer| {
            let mut modulate = layer.cover.get_modulate();
            let next = if layer.target > modulate.a {
                (modulate.a + step).min(layer.target)
            } else {
                (modulate.a - step).max(layer.target)
            };
            if next != modulate.a {
                modulate.a = next;
                layer.cover.set_modulate(modulate);
            }
            if layer.target <= 0.0 && next <= 0.0 {
                layer.cover.queue_free();
                return false;
            }
            layer.cover.bind_mut().set_clock(visible);
            true
        });
    }

    fn apply(&mut self, time: Seconds, path: PathBuf, crossfade: bool, loops: bool) {
        let alpha = if crossfade { 0.0 } else { 1.0 };
        let cover = MediaCover::instantiate(MediaCoverOptions {
            path,
            color: Color::from_rgba(DIM, DIM, DIM, alpha),
            z: 0,
            start: time,
            looping: loops,
            pace: MediaPace::Manual,
        });
        // An unshowable cue keeps the current background instead.
        let Some(cover) = cover else { return };
        for layer in &mut self.layers {
            if crossfade {
                layer.target = 0.0;
            } else {
                layer.cover.queue_free();
            }
        }
        if !crossfade {
            self.layers.clear();
        }
        self.host.add_child(&cover);
        self.layers.push(Layer { cover, target: 1.0 });
    }
}
