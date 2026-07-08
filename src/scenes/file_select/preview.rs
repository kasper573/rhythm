use super::{Wheel, WheelEntry};
use crate::core::assets::asset_server_path;
use crate::core::library::{StepfileId, StepfileLibrary};
use crate::core::scene_flow::SpawnScoped;
use crate::core::units::Seconds;
use crate::scenes::{GameScene, SceneFade};
use bevy::audio::PlaybackMode;
use bevy::prelude::*;
use std::time::Duration;

const PREVIEW_DEBOUNCE: Seconds = Seconds(0.35);

/// The wheel's music preview: the stepfile it aims at, its debounce
/// clock, and the playing audio entity.
#[derive(Resource, Default)]
pub(super) struct Preview {
    stepfile: Option<StepfileId>,
    wait: Seconds,
    entity: Option<Entity>,
}

impl Preview {
    /// Silences the preview immediately — scene teardown would stop it
    /// too, but only after the exit fade has played out.
    pub fn stop(&mut self, commands: &mut Commands) {
        if let Some(entity) = self.entity.take() {
            commands.entity(entity).try_despawn();
        }
    }
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
