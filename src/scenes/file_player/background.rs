use super::{PlaySession, PlaySet};
use crate::core::assets::asset_server_path;
use crate::core::library::{StepfileEntry, is_video_file};
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::Settings;
use crate::core::stepfile::StepfileTiming;
use crate::core::units::Seconds;
use crate::core::video::VideoStream;
use crate::core::{SCREEN_SIZE, ViewportCover, at};
use crate::scenes::GameScene;
use bevy::prelude::*;
use bevy::sprite::{SpriteImageMode, SpriteScalingMode};
use std::path::{Path, PathBuf};

pub(super) fn plugin(app: &mut App) {
    app.add_message::<BackgroundCue>()
        .add_systems(OnExit(GameScene::FilePlayer), exit)
        .add_systems(
            Update,
            (
                cue_background_changes,
                apply_background_cues,
                stream_video_frames,
            )
                .chain()
                .in_set(PlaySet::Present),
        );
}

fn exit(mut commands: Commands) {
    commands.remove_resource::<BackgroundTimeline>();
}

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
}

#[derive(Component, Default, Clone)]
pub(super) struct BackgroundLayer;

pub(super) fn spawn_background(
    commands: &mut Commands,
    entry: &StepfileEntry,
    timing: &StepfileTiming,
) {
    commands.spawn_scoped(
        GameScene::FilePlayer,
        bsn! {
            BackgroundLayer
            ViewportCover
            Sprite {
                // Dimmed so arrows and text stay readable in front of it.
                color: Color::srgb(0.5, 0.5, 0.5),
                custom_size: {Some(SCREEN_SIZE)},
                image_mode: {SpriteImageMode::Scale(SpriteScalingMode::FillCenter)},
            }
            Visibility::Hidden
        },
    );

    let changes = entry
        .stepfile
        .bg_changes
        .iter()
        .filter_map(|change| {
            let path = entry.resolve_file(&change.file)?;
            Some(BackgroundChange {
                time: timing.seconds_at_beat(change.beat),
                path,
            })
        })
        .collect();
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
}

fn cue_background_changes(
    session: Res<PlaySession>,
    settings: Res<Settings>,
    mut timeline: ResMut<BackgroundTimeline>,
    mut cues: MessageWriter<BackgroundCue>,
) {
    if let Some(path) = timeline.initial.take() {
        cues.write(BackgroundCue {
            time: Seconds::ZERO,
            path,
        });
    }
    let now = session.visible_now(&settings.timing);
    while timeline.next < timeline.changes.len() && timeline.changes[timeline.next].time.0 <= now.0
    {
        cues.write(BackgroundCue {
            time: timeline.changes[timeline.next].time,
            path: timeline.changes[timeline.next].path.clone(),
        });
        timeline.next += 1;
    }
}

fn apply_background_cues(
    mut cues: MessageReader<BackgroundCue>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    videos: Query<Entity, With<VideoStream>>,
    mut layer: Single<(&mut Sprite, &mut Visibility), With<BackgroundLayer>>,
) {
    for cue in cues.read() {
        if is_video_file(&cue.path.to_string_lossy()) {
            start_video(&mut commands, &mut images, &videos, &cue.path, cue.time);
            continue;
        }
        let Some(path) = asset_server_path(&cue.path) else {
            continue;
        };
        for video in &videos {
            commands.entity(video).despawn();
        }
        let (sprite, visibility) = &mut *layer;
        sprite.image = asset_server.load(path);
        **visibility = Visibility::Visible;
    }
}

fn stream_video_frames(
    session: Res<PlaySession>,
    settings: Res<Settings>,
    mut images: ResMut<Assets<Image>>,
    mut videos: Query<&mut VideoStream>,
) {
    let now = session.visible_now(&settings.timing);
    for mut video in &mut videos {
        video.update(now, &mut images);
    }
}

fn start_video(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    videos: &Query<Entity, With<VideoStream>>,
    path: &Path,
    start_time: Seconds,
) {
    for video in videos {
        commands.entity(video).despawn();
    }
    match VideoStream::open(path, start_time, false, images) {
        Ok(stream) => {
            let image = stream.image.clone();
            commands
                .spawn_scoped(
                    GameScene::FilePlayer,
                    bsn! {
                        ViewportCover
                        Sprite {
                            image: {image},
                            color: Color::srgb(0.6, 0.6, 0.6),
                            custom_size: {Some(SCREEN_SIZE)},
                            image_mode: {SpriteImageMode::Scale(SpriteScalingMode::FillCenter)},
                        }
                        at(0.0, 0.0, 0.5)
                    },
                )
                // The stream owns a live decoder process, so it cannot be a
                // cloneable template value.
                .insert(stream);
        }
        Err(error) => warn!(
            "video background unavailable for {}: {error}",
            path.display()
        ),
    }
}
