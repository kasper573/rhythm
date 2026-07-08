use super::{ActiveRowHighlight, Wheel, WheelEntry};
use crate::core::assets::asset_server_path;
use crate::core::config::RhythmCycle;
use crate::core::library::{StepfileId, StepfileLibrary};
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::Settings;
use crate::core::stepfile::StepfileClock;
use crate::core::units::Seconds;
use crate::scenes::{GameScene, SceneFade};
use bevy::audio::{AudioSinkPlayback, PlaybackMode};
use bevy::prelude::*;
use std::time::Duration;

const PREVIEW_DEBOUNCE: Seconds = Seconds(0.35);

/// Once every beat, apex on it, decaying cubically until the next.
const HIGHLIGHT_PULSE: RhythmCycle = RhythmCycle {
    speed: 4.0,
    easing: [0.32, 0.0, 0.67, 0.0],
};

/// The wheel's music preview: the stepfile it aims at, its debounce
/// clock, the playing audio entity, and the smooth music clock the beat
/// pulse reads.
#[derive(Resource, Default)]
pub(super) struct Preview {
    stepfile: Option<StepfileId>,
    wait: Seconds,
    entity: Option<Entity>,
    clock: Option<StepfileClock>,
}

impl Preview {
    /// Silences the preview immediately — scene teardown would stop it
    /// too, but only after the exit fade has played out.
    pub fn stop(&mut self, commands: &mut Commands) {
        if let Some(entity) = self.entity.take() {
            commands.entity(entity).try_despawn();
        }
        self.clock = None;
    }
}

/// Pulses the active-row highlight's opacity between 0.5 and 1 on the
/// preview music's beat, apex on the beat; a steady 1 while nothing plays.
pub(super) fn pulse_active_row(
    time: Res<Time>,
    settings: Res<Settings>,
    library: Res<StepfileLibrary>,
    sinks: Query<&AudioSink>,
    mut preview: ResMut<Preview>,
    mut highlight: Single<&mut Sprite, With<ActiveRowHighlight>>,
) {
    let delta = Seconds(time.delta_secs_f64());
    let beat = preview_beat(&mut preview, &library, &sinks, &settings, delta);
    let alpha = match beat {
        Some(beat) => 0.5 + 0.5 * HIGHLIGHT_PULSE.strike(beat),
        None => 1.0,
    };
    if highlight.color.alpha() != alpha {
        highlight.color.set_alpha(alpha);
    }
}

/// The beat the speakers are on, through the same [`StepfileClock`] the
/// gameplay scene grades and draws with. The mixer's raw position keeps
/// growing while the sample loops, so it is folded back into the loop
/// window first — the servo's resync snap absorbs the seam, keeping the
/// pulse locked to what is audibly playing.
fn preview_beat(
    preview: &mut Preview,
    library: &StepfileLibrary,
    sinks: &Query<&AudioSink>,
    settings: &Settings,
    delta: Seconds,
) -> Option<f64> {
    let id = preview.stepfile?;
    let sink = sinks.get(preview.entity?).ok()?;
    let stepfile = &library.stepfile(id).stepfile;
    let report = stepfile.sample_position(Seconds(sink.position().as_secs_f64()));
    let clock = preview
        .clock
        .get_or_insert_with(|| StepfileClock::start_at(stepfile.timing.clone(), report));
    clock.advance(delta, Some(report));
    Some(clock.visible_beat(&settings.timing).0)
}

/// Follows the active wheel row with a debounce, then loops the
/// stepfile's sample range. Falls silent the moment any scene transition
/// starts, instead of playing through the fade.
pub(super) fn update_preview(
    time: Res<Time>,
    wheel: Res<Wheel>,
    mut preview: ResMut<Preview>,
    library: Res<StepfileLibrary>,
    asset_server: Res<AssetServer>,
    fade: Res<SceneFade>,
    mut commands: Commands,
) {
    if !fade.accepts_input() {
        preview.stop(&mut commands);
        return;
    }
    let active_stepfile = match wheel.entries.get(wheel.active) {
        Some(WheelEntry::Stepfile { id }) => Some(*id),
        _ => None,
    };

    if preview.stepfile != active_stepfile {
        preview.stepfile = active_stepfile;
        preview.wait = Seconds::ZERO;
        preview.stop(&mut commands);
        return;
    }

    let Some(id) = active_stepfile else { return };
    if preview.entity.is_some() {
        return;
    }
    preview.wait += Seconds(time.delta_secs_f64());
    if preview.wait < PREVIEW_DEBOUNCE {
        return;
    }

    let entry = library.stepfile(id);
    let Some(path) = entry.music_path().as_deref().and_then(asset_server_path) else {
        return;
    };
    let stepfile = &entry.stepfile;
    let start = stepfile.sample_start.0.max(0.0);
    let length = stepfile.sample_length.0;
    let music = asset_server.load(path);
    let entity = commands
        .spawn_scoped(
            GameScene::FileSelect,
            bsn! {
                AudioPlayer({music})
                PlaybackSettings {
                    mode: {PlaybackMode::Loop},
                    start_position: {Some(Duration::from_secs_f64(start))},
                    duration: {(length > 0.0).then(|| Duration::from_secs_f64(length))},
                }
            },
        )
        .id();
    preview.entity = Some(entity);
}
