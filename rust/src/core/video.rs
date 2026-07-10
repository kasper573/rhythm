use crate::core::platform::{VideoSource, platform};
use crate::core::units::Seconds;
use godot::classes::image::Format;
use godot::classes::{Image, ImageTexture};
use godot::prelude::*;
use std::path::Path;

/// Streams a video file into an [`ImageTexture`], decoded by the installed
/// [`Platform`](crate::core::platform::Platform). Playback position is
/// driven by the caller's clock, so video stays locked to the music. When
/// the platform cannot decode the file, opening fails and callers keep
/// their static background instead.
pub struct VideoStream {
    pub texture: Gd<ImageTexture>,
    /// Clock time at which the video starts.
    start_time: Seconds,
    source: Box<dyn VideoSource>,
    size: (u32, u32),
}

impl VideoStream {
    pub fn open(path: &Path, start_time: Seconds, looping: bool) -> Result<VideoStream, String> {
        let source = platform().open_video(path, looping)?;
        // A placeholder until the first frame arrives: some platforms only
        // learn a video's dimensions after opening it.
        let placeholder = frame_image(1, 1, &[0, 0, 0, 255]);
        let texture =
            ImageTexture::create_from_image(&placeholder).ok_or("cannot create a texture")?;
        Ok(VideoStream {
            texture,
            start_time,
            source,
            size: (1, 1),
        })
    }

    /// Advances the texture to the frame that should be visible at `clock`.
    pub fn update(&mut self, clock: Seconds) {
        let Some(frame) = self.source.poll(clock - self.start_time) else {
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
