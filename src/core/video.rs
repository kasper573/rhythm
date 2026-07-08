use crate::core::units::Seconds;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

/// Streams a video file into an [`Image`] asset, decoded in-process by
/// `video-rs` on a worker thread. Playback position is driven by the
/// caller's clock, so video stays locked to the music; late frames are
/// dropped. When the file cannot be decoded, opening fails and callers
/// keep their static background instead.
#[derive(Component)]
pub struct VideoStream {
    pub image: Handle<Image>,
    pub width: u32,
    pub height: u32,
    frames_per_second: f64,
    /// Clock time at which frame zero is displayed.
    start_time: Seconds,
    frames_shown: i64,
    frames: Mutex<Receiver<Vec<u8>>>,
}

impl VideoStream {
    pub fn open(
        path: &Path,
        start_time: Seconds,
        looping: bool,
        images: &mut Assets<Image>,
    ) -> Result<VideoStream, String> {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            if let Err(error) = video_rs::init() {
                warn!("video subsystem init failed: {error}");
            }
        });
        let decoder = video_rs::Decoder::new(path).map_err(|error| error.to_string())?;
        let (width, height) = decoder.size();
        let frames_per_second = decoder.frame_rate() as f64;
        if width == 0 || height == 0 || frames_per_second <= 0.0 {
            return Err("video reports no dimensions or frame rate".to_string());
        }

        let (sender, frames) = sync_channel(QUEUE_LIMIT);
        let path = path.to_path_buf();
        std::thread::spawn(move || decode(decoder, path, looping, &sender));

        let image = images.add(Image::new_fill(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 255],
            TextureFormat::Rgba8UnormSrgb,
            Default::default(),
        ));

        Ok(VideoStream {
            image,
            width,
            height,
            frames_per_second,
            start_time,
            frames_shown: 0,
            frames: Mutex::new(frames),
        })
    }

    /// Advances the image to the frame that should be visible at `clock`,
    /// skipping frames when behind. Holds the last frame once the video ends.
    pub fn update(&mut self, clock: Seconds, images: &mut Assets<Image>) {
        let target = ((clock - self.start_time).0 * self.frames_per_second).floor() as i64;
        let mut latest = None;
        let queue = self.frames.lock().expect("video frame queue poisoned");
        while self.frames_shown <= target {
            let Ok(frame) = queue.try_recv() else { break };
            self.frames_shown += 1;
            latest = Some(frame);
        }
        drop(queue);
        if let Some(frame) = latest
            && let Some(mut image) = images.get_mut(&self.image)
        {
            image.data = Some(frame);
        }
    }
}

/// Small buffer of decoded frames; the bounded channel back-pressures the
/// decoder thread, so the whole video never sits in memory. Dropping the
/// stream disconnects the channel and the thread winds down on its next
/// send.
const QUEUE_LIMIT: usize = 8;

/// Feeds decoded frames as RGBA until the receiver is dropped, or until
/// the file ends when not looping.
fn decode(
    mut decoder: video_rs::Decoder,
    path: PathBuf,
    looping: bool,
    frames: &SyncSender<Vec<u8>>,
) {
    loop {
        for frame in decoder.decode_iter() {
            let Ok((_, frame)) = frame else { break };
            let Some(rgb) = frame.as_slice() else { break };
            let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
            for pixel in rgb.chunks_exact(3) {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
            if frames.send(rgba).is_err() {
                return;
            }
        }
        if !looping {
            return;
        }
        // A fresh decoder per lap, instead of seeking the same one back
        // and flushing codec state by hand.
        match video_rs::Decoder::new(path.as_path()) {
            Ok(next) => decoder = next,
            Err(_) => return,
        }
    }
}
