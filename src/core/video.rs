use crate::core::platform::{VideoSource, platform};
use crate::core::units::Seconds;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::path::Path;

/// Streams a video file into an [`Image`] asset, decoded by the installed
/// [`Platform`](crate::core::platform::Platform). Playback position is
/// driven by the caller's clock, so video stays locked to the music. When
/// the platform cannot decode the file, opening fails and callers keep
/// their static background instead.
#[derive(Component)]
pub struct VideoStream {
    pub image: Handle<Image>,
    /// Clock time at which the video starts.
    start_time: Seconds,
    source: Box<dyn VideoSource>,
    size: (u32, u32),
}

impl VideoStream {
    pub fn open(
        path: &Path,
        start_time: Seconds,
        looping: bool,
        images: &mut Assets<Image>,
    ) -> Result<VideoStream, String> {
        let source = platform().open_video(path, looping)?;
        // A placeholder until the first frame arrives: some platforms only
        // learn a video's dimensions after opening it.
        let image = images.add(sized_image(1, 1, vec![0, 0, 0, 255]));
        Ok(VideoStream {
            image,
            start_time,
            source,
            size: (1, 1),
        })
    }

    /// Advances the image to the frame that should be visible at `clock`.
    pub fn update(&mut self, clock: Seconds, images: &mut Assets<Image>) {
        let Some(frame) = self.source.poll(clock - self.start_time) else {
            return;
        };
        let Some(mut image) = images.get_mut(&self.image) else {
            return;
        };
        if self.size == (frame.width, frame.height) {
            image.data = Some(frame.rgba);
        } else {
            self.size = (frame.width, frame.height);
            *image = sized_image(frame.width, frame.height, frame.rgba);
        }
    }
}

fn sized_image(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        Default::default(),
    )
}
