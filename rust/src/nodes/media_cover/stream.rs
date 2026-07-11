//! The engine-facing half of a video cover: a [`VideoStream`] resource and
//! [`VideoStreamPlayback`] pair over the platform's decoder. The cover
//! drives its playback itself on the main thread — [`VideoStreamPlayer`]
//! queries playbacks from the audio mix thread, which a single-threaded
//! extension class must never meet, and the play stage needs video locked
//! to the music clock rather than the engine clock anyway.
//!
//! [`VideoStreamPlayer`]: godot::classes::VideoStreamPlayer

use crate::core::platform::{VideoSource, platform};
use crate::core::units::Seconds;
use godot::classes::image::Format;
use godot::classes::{
    IVideoStream, IVideoStreamPlayback, Image, ImageTexture, Texture2D, VideoStream,
    VideoStreamPlayback,
};
use godot::prelude::*;
use std::path::Path;

/// A video stream over the platform decoder, opened for one specific file.
#[derive(GodotClass)]
#[class(base=VideoStream)]
pub struct MediaVideoStream {
    /// Waiting for the playback to start; consumed then.
    source: Option<Box<dyn VideoSource>>,
    base: Base<VideoStream>,
}

#[godot_api]
impl MediaVideoStream {
    /// Opens the file with the platform decoder; failing that — an absent
    /// file, a codec the platform cannot play — reports why, so the owner
    /// can keep showing what it already has.
    pub fn open(path: &Path, looping: bool) -> Result<Gd<MediaVideoStream>, String> {
        let source = platform().open_video(path, looping)?;
        let mut stream = MediaVideoStream::new_gd();
        stream.bind_mut().source = Some(source);
        Ok(stream)
    }

    /// Starts this stream's playback for an owner that drives it.
    pub fn start_playback(&mut self) -> Option<Gd<MediaVideoPlayback>> {
        let source = self.source.take()?;
        let mut playback = MediaVideoPlayback::create(source);
        playback.bind_mut().playing = true;
        Some(playback)
    }
}

#[godot_api]
impl IVideoStream for MediaVideoStream {
    fn init(base: Base<VideoStream>) -> MediaVideoStream {
        MediaVideoStream { source: None, base }
    }

    fn instantiate_playback(&mut self) -> Option<Gd<VideoStreamPlayback>> {
        self.start_playback().map(Gd::upcast)
    }
}

/// Streams decoded frames into a texture as the engine's video player
/// updates it. The sources loop internally, so playback never finishes on
/// its own.
#[derive(GodotClass)]
#[class(no_init, base=VideoStreamPlayback)]
pub struct MediaVideoPlayback {
    source: Box<dyn VideoSource>,
    texture: Gd<ImageTexture>,
    size: (u32, u32),
    /// Media time in seconds: self-advancing, unless an external clock
    /// overrides it every frame.
    position: f64,
    external: Option<f64>,
    playing: bool,
    base: Base<VideoStreamPlayback>,
}

impl MediaVideoPlayback {
    /// One frame of playback: advance the clock — or adopt the external
    /// one — and stream the due frame into the texture.
    pub fn advance(&mut self, delta: f64) {
        if !self.playing {
            return;
        }
        match self.external {
            Some(time) => self.position = time,
            None => self.position += delta,
        }
        let Some(frame) = self.source.poll(Seconds(self.position)) else {
            return;
        };
        let image = frame_image(frame.width, frame.height, &frame.rgba);
        if self.size == (frame.width, frame.height) {
            self.texture.update(&image);
        } else {
            self.size = (frame.width, frame.height);
            self.texture.set_image(&image);
        }
    }
}

#[godot_api]
impl MediaVideoPlayback {
    fn create(source: Box<dyn VideoSource>) -> Gd<MediaVideoPlayback> {
        // A placeholder until the first frame arrives: some platforms only
        // learn a video's dimensions after opening it.
        let placeholder = frame_image(1, 1, &[0, 0, 0, 255]);
        let texture =
            ImageTexture::create_from_image(&placeholder).expect("an image makes a texture");
        Gd::from_init_fn(|base| MediaVideoPlayback {
            source,
            texture,
            size: (1, 1),
            position: 0.0,
            external: None,
            playing: false,
            base,
        })
    }

    /// Locks playback to an external media time — the owner writes it
    /// every frame and the engine clock is ignored.
    pub fn set_clock(&mut self, media_time: Seconds) {
        self.external = Some(media_time.0);
    }

    /// The texture frames stream into; stable for the playback's lifetime.
    pub fn texture(&self) -> Gd<ImageTexture> {
        self.texture.clone()
    }
}

#[godot_api]
impl IVideoStreamPlayback for MediaVideoPlayback {
    fn play(&mut self) {
        self.playing = true;
    }

    fn stop(&mut self) {
        self.playing = false;
    }

    fn is_playing(&self) -> bool {
        self.playing
    }

    fn set_paused(&mut self, paused: bool) {
        self.playing = !paused;
    }

    fn is_paused(&self) -> bool {
        !self.playing
    }

    /// Unknown: the platform sources expose no duration, and the covers
    /// loop forever anyway.
    fn get_length(&self) -> f64 {
        0.0
    }

    fn get_playback_position(&self) -> f64 {
        self.position
    }

    fn seek(&mut self, time: f64) {
        self.position = time;
    }

    fn update(&mut self, delta: f64) {
        self.advance(delta);
    }

    fn get_texture(&self) -> Option<Gd<Texture2D>> {
        Some(self.texture.clone().upcast())
    }

    fn get_channels(&self) -> i32 {
        0
    }

    fn get_mix_rate(&self) -> i32 {
        0
    }
}

fn frame_image(width: u32, height: u32, rgba: &[u8]) -> Gd<Image> {
    Image::create_from_data(
        width as i32,
        height as i32,
        false,
        Format::RGBA8,
        &PackedByteArray::from(rgba),
    )
    .expect("video frames are valid RGBA images")
}
