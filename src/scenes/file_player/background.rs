use super::{PlaySet, PlayTime};
use crate::core::assets::asset_server_path;
use crate::core::library::{StepfileEntry, is_video_file};
use crate::core::scene_flow::SpawnScoped;
use crate::core::stepfile::StepfileTiming;
use crate::core::units::Seconds;
use crate::core::video::VideoStream;
use crate::core::{SCREEN_SIZE, ViewportCover, at};
use crate::scenes::GameScene;
use bevy::prelude::*;
use bevy::sprite::{SpriteImageMode, SpriteScalingMode};
use std::path::PathBuf;

pub(super) fn plugin(app: &mut App) {
    app.add_message::<BackgroundCue>()
        .add_systems(OnExit(GameScene::FilePlayer), exit)
        .add_systems(
            Update,
            (
                cue_background_changes,
                apply_background_cues,
                fade_background_layers,
                stream_video_frames,
            )
                .chain()
                .in_set(PlaySet::Present)
                .run_if(in_state(GameScene::FilePlayer)),
        );
}

fn exit(mut commands: Commands) {
    commands.remove_resource::<BackgroundTimeline>();
}

/// How long a `CrossFade` transition blends between backgrounds.
const CROSSFADE_SECONDS: f32 = 0.5;

/// Dimmed so arrows and text stay readable in front of the background.
const DIM: f32 = 0.5;

/// Background switches from the stepfile's `#BGCHANGES`, resolved to files
/// that actually exist, ordered by time.
#[derive(Resource)]
struct BackgroundTimeline {
    /// The stepfile's own background, shown before any timed change.
    initial: Option<PathBuf>,
    changes: Vec<BackgroundChange>,
    next: usize,
}

struct BackgroundChange {
    time: Seconds,
    path: PathBuf,
    crossfade: bool,
    loops: bool,
}

/// One background on screen — image or video — easing toward its target
/// opacity; fully faded-out layers retire.
#[derive(Component)]
struct BackgroundLayer {
    target: f32,
}

pub(super) fn spawn_background(
    commands: &mut Commands,
    entry: &StepfileEntry,
    timing: &StepfileTiming,
) {
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
    commands.insert_resource(BackgroundTimeline {
        initial: entry.background_path(),
        changes,
        next: 0,
    });
}

/// A background change whose time has come, recognized on the musical
/// timeline and applied separately.
#[derive(Message)]
struct BackgroundCue {
    time: Seconds,
    path: PathBuf,
    crossfade: bool,
    loops: bool,
}

fn cue_background_changes(
    play_time: Res<PlayTime>,
    mut timeline: ResMut<BackgroundTimeline>,
    mut cues: MessageWriter<BackgroundCue>,
) {
    if let Some(path) = timeline.initial.take() {
        cues.write(BackgroundCue {
            time: Seconds::ZERO,
            path,
            crossfade: false,
            loops: false,
        });
    }
    let now = play_time.visible;
    while timeline.next < timeline.changes.len() && timeline.changes[timeline.next].time.0 <= now.0
    {
        let change = &timeline.changes[timeline.next];
        cues.write(BackgroundCue {
            time: change.time,
            path: change.path.clone(),
            crossfade: change.crossfade,
            loops: change.loops,
        });
        timeline.next += 1;
    }
}

fn apply_background_cues(
    mut cues: MessageReader<BackgroundCue>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut layers: Query<(Entity, &mut BackgroundLayer)>,
    mut layer_count: Local<u32>,
) {
    for cue in cues.read() {
        // The incoming background: a video stream, or a loaded image.
        let (image, stream) = if is_video_file(&cue.path.to_string_lossy()) {
            match VideoStream::open(&cue.path, cue.time, cue.loops, &mut images) {
                Ok(stream) => (stream.image.clone(), Some(stream)),
                Err(error) => {
                    warn!(
                        "video background unavailable for {}: {error}",
                        cue.path.display()
                    );
                    continue;
                }
            }
        } else {
            let Some(path) = asset_server_path(&cue.path) else {
                continue;
            };
            (asset_server.load(path), None)
        };

        for (entity, mut layer) in &mut layers {
            if cue.crossfade {
                layer.target = 0.0;
            } else {
                commands.entity(entity).despawn();
            }
        }

        // Newer layers draw above the ones fading out; the small cycling
        // bump stays below the note field.
        *layer_count += 1;
        let z = 0.3 + (*layer_count % 100) as f32 * 0.002;
        let alpha = if cue.crossfade { 0.0 } else { 1.0 };
        let mut layer = commands.spawn_scoped(
            GameScene::FilePlayer,
            bsn! {
                ViewportCover
                Sprite {
                    image: {image},
                    color: {Color::srgba(DIM, DIM, DIM, alpha)},
                    custom_size: {Some(SCREEN_SIZE)},
                    image_mode: {SpriteImageMode::Scale(SpriteScalingMode::FillCenter)},
                }
                at(0.0, 0.0, z)
            },
        );
        layer.insert(BackgroundLayer { target: 1.0 });
        // The stream owns a live decoder, so it cannot be a cloneable
        // template value.
        if let Some(stream) = stream {
            layer.insert(stream);
        }
    }
}

/// Runs every layer's timed linear blend and retires the faded-out ones.
fn fade_background_layers(
    time: Res<Time>,
    mut layers: Query<(Entity, &BackgroundLayer, &mut Sprite)>,
    mut commands: Commands,
) {
    let step = time.delta_secs() / CROSSFADE_SECONDS;
    for (entity, layer, mut sprite) in &mut layers {
        let alpha = sprite.color.alpha();
        let next = if layer.target > alpha {
            (alpha + step).min(layer.target)
        } else {
            (alpha - step).max(layer.target)
        };
        if next != alpha {
            sprite.color.set_alpha(next);
        }
        if layer.target <= 0.0 && next <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn stream_video_frames(
    play_time: Res<PlayTime>,
    mut images: ResMut<Assets<Image>>,
    mut videos: Query<&mut VideoStream>,
) {
    let now = play_time.visible;
    for mut video in &mut videos {
        video.update(now, &mut images);
    }
}
