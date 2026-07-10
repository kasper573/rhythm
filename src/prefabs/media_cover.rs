use crate::core::assets::asset_server_path;
use crate::core::library::is_video_file;
use crate::core::units::Seconds;
use crate::core::video::VideoStream;
use crate::core::{SCREEN_SIZE, ViewportCover, at};
use bevy::prelude::*;
use bevy::sprite::{SpriteImageMode, SpriteScalingMode};
use std::path::PathBuf;

pub struct MediaCoverPrefabOptions {
    /// Absolute path to the image or video file to show.
    pub path: PathBuf,
    /// Sprite tint; dims the media and carries the cover's opacity.
    pub color: Color,
    /// World z the cover draws at.
    pub z: f32,
    /// The clock moment the media starts at (videos begin here).
    pub start: Seconds,
    /// Whether a video loops; images ignore this.
    pub looping: bool,
    pub pace: MediaPace,
}

/// A viewport-covering sprite showing a media file — a still image or a
/// streaming video — scaled to fill and cropped, never stretched. The one
/// way scenes put a full-screen background on stage.
///
/// The prefab renders; the owner orchestrates: fade or retire the cover
/// through its `Sprite`, and — under [`MediaPace::Manual`] — pace a video
/// by writing [`MediaCover::clock`] every frame. Returns `None` (after a
/// warning) when the media cannot be shown, so callers can keep whatever
/// they already display.
pub fn media_cover_prefab(
    opt: MediaCoverPrefabOptions,
    commands: &mut Commands,
    asset_server: &AssetServer,
    images: &mut Assets<Image>,
) -> Option<Entity> {
    let (image, stream) = if is_video_file(&opt.path.to_string_lossy()) {
        match VideoStream::open(&opt.path, opt.start, opt.looping, images) {
            Ok(stream) => (stream.image.clone(), Some(stream)),
            Err(error) => {
                warn!(
                    "media cover unavailable for {}: {error}",
                    opt.path.display()
                );
                return None;
            }
        }
    } else {
        let Some(path) = asset_server_path(&opt.path) else {
            warn!(
                "media cover unavailable: {} is outside the asset root",
                opt.path.display()
            );
            return None;
        };
        (asset_server.load(path), None)
    };
    let color = opt.color;
    let z = opt.z;
    let mut cover = commands.spawn_scene(bsn! {
        ViewportCover
        Sprite {
            image: {image},
            color: {color},
            custom_size: {Some(SCREEN_SIZE)},
            image_mode: {SpriteImageMode::Scale(SpriteScalingMode::FillCenter)},
        }
        at(0.0, 0.0, z)
    });
    cover.insert(MediaCover {
        clock: opt.start,
        pace: opt.pace,
    });
    // The stream owns a live decoder, so it cannot be a cloneable template
    // value.
    if let Some(stream) = stream {
        cover.insert(stream);
    }
    Some(cover.id())
}

/// What paces a cover's playback clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPace {
    /// Wall time — for backgrounds that simply play.
    Wall,
    /// The owner writes [`MediaCover::clock`] every frame — for backgrounds
    /// locked to another timeline, like the play stage's music.
    Manual,
}

/// A spawned cover's playback clock, in the media's own timeline.
#[derive(Component)]
pub struct MediaCover {
    pub clock: Seconds,
    pace: MediaPace,
}

pub struct MediaCoverPlugin;

impl Plugin for MediaCoverPlugin {
    fn build(&self, app: &mut App) {
        // PostUpdate, so manually paced clocks written during Update reach
        // the video the same frame.
        app.add_systems(PostUpdate, stream_media_covers);
    }
}

fn stream_media_covers(
    time: Res<Time>,
    mut images: ResMut<Assets<Image>>,
    mut covers: Query<(&mut MediaCover, Option<&mut VideoStream>)>,
) {
    for (mut cover, stream) in &mut covers {
        if cover.pace == MediaPace::Wall {
            cover.clock = Seconds(time.elapsed_secs_f64());
        }
        if let Some(mut stream) = stream {
            stream.update(cover.clock, &mut images);
        }
    }
}
