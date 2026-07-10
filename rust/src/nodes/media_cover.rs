use crate::core::library::is_video_file;
use crate::core::platform::{AssetFetch, FetchPoll, platform};
use crate::core::textures::decode_texture;
use crate::core::units::Seconds;
use crate::core::video::VideoStream;
use godot::classes::control::LayoutPreset;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{ITextureRect, TextureRect};
use godot::global::godot_warn;
use godot::prelude::*;
use std::path::PathBuf;

pub struct MediaCoverOptions {
    /// Absolute path to the image or video file to show.
    pub path: PathBuf,
    /// Modulation; dims the media and carries the cover's opacity.
    pub color: Color,
    /// Canvas z-index the cover draws at.
    pub z: i32,
    /// The clock moment the media starts at (videos begin here).
    pub start: Seconds,
    /// Whether a video loops; images ignore this.
    pub looping: bool,
    pub pace: MediaPace,
}

/// A viewport-covering surface showing a media file — a still image or a
/// streaming video — scaled to fill and cropped, never stretched. The one
/// way scenes put a full-screen background on stage.
///
/// The node renders; the owner orchestrates: fade or retire the cover
/// through its modulate, and — under [`MediaPace::Manual`] — pace a video
/// by writing [`set_clock`](MediaCover::set_clock) every frame. Instantiation
/// returns `None` (after a warning) when the media cannot be shown, so
/// callers can keep whatever they already display.
#[derive(GodotClass)]
#[class(base=TextureRect)]
pub struct MediaCover {
    clock: Seconds,
    pace: MediaPace,
    stream: Option<VideoStream>,
    fetch: Option<Box<dyn AssetFetch>>,
    image_path: PathBuf,
    ready: bool,
    base: Base<TextureRect>,
}

/// What paces a cover's playback clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPace {
    /// Wall time — for backgrounds that simply play.
    Wall,
    /// The owner writes the clock every frame — for backgrounds locked to
    /// another timeline, like the play stage's music.
    Manual,
}

#[godot_api]
impl MediaCover {
    pub fn instantiate(opt: MediaCoverOptions) -> Option<Gd<MediaCover>> {
        let mut cover = MediaCover::new_alloc();
        cover.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        cover.set_stretch_mode(StretchMode::KEEP_ASPECT_COVERED);
        cover.set_expand_mode(ExpandMode::IGNORE_SIZE);
        cover.set_modulate(opt.color);
        cover.set_z_index(opt.z);
        cover.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);

        let mut bound = cover.bind_mut();
        bound.clock = opt.start;
        bound.pace = opt.pace;
        if is_video_file(&opt.path.to_string_lossy()) {
            match VideoStream::open(&opt.path, opt.start, opt.looping) {
                Ok(stream) => {
                    let texture = stream.texture.clone();
                    bound.stream = Some(stream);
                    bound.ready = true;
                    drop(bound);
                    cover.set_texture(&texture);
                }
                Err(error) => {
                    godot_warn!(
                        "media cover unavailable for {}: {error}",
                        opt.path.display()
                    );
                    drop(bound);
                    cover.queue_free();
                    return None;
                }
            }
        } else {
            bound.fetch = Some(platform().fetch_asset(&opt.path));
            bound.image_path = opt.path;
            drop(bound);
        }
        Some(cover)
    }

    /// Whether the cover has real pixels to show — a decoded video or a
    /// loaded image. Owners cross-fading layers wait for this before
    /// retiring what is underneath.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Drives a [`MediaPace::Manual`] cover's playback clock.
    pub fn set_clock(&mut self, clock: Seconds) {
        self.clock = clock;
    }
}

#[godot_api]
impl ITextureRect for MediaCover {
    fn init(base: Base<TextureRect>) -> MediaCover {
        MediaCover {
            clock: Seconds::ZERO,
            pace: MediaPace::Wall,
            stream: None,
            fetch: None,
            image_path: PathBuf::new(),
            ready: false,
            base,
        }
    }

    fn process(&mut self, delta: f64) {
        if let Some(fetch) = &mut self.fetch {
            match fetch.poll() {
                FetchPoll::Pending => {}
                FetchPoll::Failed(error) => {
                    godot_warn!("media cover image failed: {error}");
                    self.fetch = None;
                }
                FetchPoll::Ready(bytes) => {
                    self.fetch = None;
                    match decode_texture(&self.image_path, &bytes) {
                        Some(texture) => {
                            self.ready = true;
                            self.base_mut().set_texture(&texture);
                        }
                        None => godot_warn!(
                            "media cover image cannot decode: {}",
                            self.image_path.display()
                        ),
                    }
                }
            }
        }
        if self.pace == MediaPace::Wall {
            self.clock += Seconds(delta);
        }
        let clock = self.clock;
        if let Some(stream) = &mut self.stream {
            stream.update(clock);
        }
    }
}
