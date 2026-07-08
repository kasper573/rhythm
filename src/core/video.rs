use crate::core::units::Seconds;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

/// Streams a video file into an [`Image`] asset by decoding frames through an
/// `ffmpeg` subprocess. Playback position is driven by the caller's clock, so
/// video stays locked to the music; late frames are dropped. When ffmpeg or
/// ffprobe are unavailable, opening fails and callers keep their static
/// background instead.
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
    process: Child,
}

impl VideoStream {
    pub fn open(
        path: &Path,
        start_time: Seconds,
        looping: bool,
        images: &mut Assets<Image>,
    ) -> Result<VideoStream, String> {
        let (width, height, frames_per_second) = probe(path)?;
        let mut process = Command::new("ffmpeg")
            .args(["-loglevel", "quiet"])
            .args(if looping {
                &["-stream_loop", "-1"][..]
            } else {
                &[][..]
            })
            .arg("-i")
            .arg(path)
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("could not start ffmpeg: {error}"))?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| "ffmpeg stdout unavailable".to_string())?;

        let (sender, frames) = sync_channel(QUEUE_LIMIT);
        let frame_size = (width * height * 4) as usize;
        std::thread::spawn(move || read_frames(stdout, frame_size, &sender));

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
            process,
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

impl Drop for VideoStream {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// Small buffer of decoded frames; the bounded channel back-pressures
/// ffmpeg, so the whole video never sits in memory.
const QUEUE_LIMIT: usize = 8;

fn read_frames(mut stdout: impl Read, frame_size: usize, frames: &SyncSender<Vec<u8>>) {
    loop {
        let mut frame = vec![0u8; frame_size];
        if stdout.read_exact(&mut frame).is_err() || frames.send(frame).is_err() {
            return;
        }
    }
}

fn probe(path: &Path) -> Result<(u32, u32, f64), String> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,avg_frame_rate",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .map_err(|error| format!("could not run ffprobe: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut fields = text.trim().split(',');
    let width: u32 = fields
        .next()
        .and_then(|field| field.parse().ok())
        .ok_or_else(|| format!("ffprobe gave no width for {}", path.display()))?;
    let height: u32 = fields
        .next()
        .and_then(|field| field.parse().ok())
        .ok_or_else(|| format!("ffprobe gave no height for {}", path.display()))?;
    let frames_per_second = fields
        .next()
        .and_then(parse_frame_rate)
        .ok_or_else(|| format!("ffprobe gave no frame rate for {}", path.display()))?;
    Ok((width, height, frames_per_second))
}

fn parse_frame_rate(fraction: &str) -> Option<f64> {
    let (numerator, denominator) = fraction.trim().split_once('/')?;
    let numerator: f64 = numerator.parse().ok()?;
    let denominator: f64 = denominator.parse().ok()?;
    (denominator > 0.0 && numerator > 0.0).then(|| numerator / denominator)
}
