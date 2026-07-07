use super::PlaySession;
use crate::core::assets::asset_server_path;
use crate::core::library::{StepfileEntry, is_video_file};
use crate::core::scene_flow::GameScene;
use crate::core::settings::Settings;
use crate::core::stepfile::StepfileTiming;
use crate::core::units::Seconds;
use crate::core::video::VideoStream;
use bevy::prelude::*;
use std::path::{Path, PathBuf};

/// Background switches from the stepfile's `#BGCHANGES`, resolved to files
/// that actually exist, ordered by time.
#[derive(Resource)]
pub(super) struct BackgroundTimeline {
    changes: Vec<BackgroundChange>,
    next: usize,
}

struct BackgroundChange {
    time: Seconds,
    path: PathBuf,
}

#[derive(Component)]
pub(super) struct BackgroundLayer;

pub(super) fn spawn_background(
    commands: &mut Commands,
    asset_server: &AssetServer,
    entry: &StepfileEntry,
    timing: &StepfileTiming,
) {
    let initial = entry
        .background_path()
        .as_deref()
        .and_then(asset_server_path)
        .map(|path| asset_server.load(path));
    let visibility = match initial {
        Some(_) => Visibility::Visible,
        None => Visibility::Hidden,
    };
    commands.spawn((
        DespawnOnExit(GameScene::FilePlayer),
        BackgroundLayer,
        Sprite {
            image: initial.unwrap_or_default(),
            // Dimmed so arrows and text stay readable in front of it.
            color: Color::srgb(0.5, 0.5, 0.5),
            custom_size: Some(Vec2::new(1280.0, 720.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0),
        visibility,
    ));

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
    commands.insert_resource(BackgroundTimeline { changes, next: 0 });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_background_changes(
    session: Res<PlaySession>,
    settings: Res<Settings>,
    mut timeline: ResMut<BackgroundTimeline>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    videos: Query<Entity, With<VideoStream>>,
    mut layer: Single<(&mut Sprite, &mut Visibility), With<BackgroundLayer>>,
) {
    let now = session.visible_now(&settings.timing);
    while timeline.next < timeline.changes.len() && timeline.changes[timeline.next].time.0 <= now.0
    {
        let change_time = timeline.changes[timeline.next].time;
        let change_path = timeline.changes[timeline.next].path.clone();
        timeline.next += 1;
        if is_video_file(&change_path.to_string_lossy()) {
            start_video(
                &mut commands,
                &mut images,
                &videos,
                &change_path,
                change_time,
            );
            continue;
        }
        let Some(path) = asset_server_path(&change_path) else {
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

/// Pulls decoded frames into the video texture, paced by the music clock.
pub(super) fn stream_video_frames(
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
    match VideoStream::open(path, start_time, images) {
        Ok(stream) => {
            // Fit inside the window, preserving the video's aspect ratio.
            let scale = (1280.0 / stream.width as f32).min(720.0 / stream.height as f32);
            let size = Vec2::new(stream.width as f32, stream.height as f32) * scale;
            commands.spawn((
                DespawnOnExit(GameScene::FilePlayer),
                Sprite {
                    image: stream.image.clone(),
                    color: Color::srgb(0.6, 0.6, 0.6),
                    custom_size: Some(size),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, 0.5),
                stream,
            ));
        }
        Err(error) => warn!(
            "video background unavailable for {}: {error}",
            path.display()
        ),
    }
}
