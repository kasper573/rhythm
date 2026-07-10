use crate::core::platform::{AssetEntry, AssetFetch, FetchPoll, Platform, VideoFrame, VideoSource};
use crate::core::units::Seconds;
use godot::classes::{Os, ProjectSettings};
use godot::global::godot_warn;
use godot::obj::Singleton;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

/// The desktop host: assets on the real filesystem, video decoded
/// in-process by `video-rs` on a worker thread.
pub struct NativePlatform;

impl Platform for NativePlatform {
    /// `RHYTHM_ASSET_ROOT` wins; otherwise dev runs (from the Godot
    /// project) use the repository's `assets/` next to the project folder,
    /// and exported builds the `assets/` folder next to the executable.
    fn asset_root(&self) -> PathBuf {
        if let Ok(root) = std::env::var("RHYTHM_ASSET_ROOT") {
            return PathBuf::from(root).join("assets");
        }
        if Os::singleton().has_feature("editor") {
            let project: PathBuf = ProjectSettings::singleton()
                .globalize_path("res://")
                .to_string()
                .into();
            if let Some(repo) = project.parent() {
                return repo.join("assets");
            }
        }
        let executable = PathBuf::from(Os::singleton().get_executable_path().to_string());
        executable
            .parent()
            .expect("executable has a parent directory")
            .join("assets")
    }

    fn read_asset(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn list_asset_dir(&self, dir: &Path) -> Vec<AssetEntry> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .map(|entry| {
                let path = entry.path();
                AssetEntry {
                    is_dir: path.is_dir(),
                    path,
                }
            })
            .collect()
    }

    fn asset_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn fetch_asset(&self, path: &Path) -> Box<dyn AssetFetch> {
        Box::new(NativeFetch {
            result: Some(std::fs::read(path)),
        })
    }

    fn open_video(&self, path: &Path, looping: bool) -> Result<Box<dyn VideoSource>, String> {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            if let Err(error) = video_rs::init() {
                godot_warn!("video subsystem init failed: {error}");
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

        Ok(Box::new(NativeVideoSource {
            width,
            height,
            frames_per_second,
            frames_shown: 0,
            frames: Mutex::new(frames),
        }))
    }
}

/// Disk reads resolve on the spot; the fetch is ready at its first poll.
struct NativeFetch {
    result: Option<io::Result<Vec<u8>>>,
}

impl AssetFetch for NativeFetch {
    fn poll(&mut self) -> FetchPoll {
        match self.result.take() {
            Some(Ok(bytes)) => FetchPoll::Ready(bytes),
            Some(Err(error)) => FetchPoll::Failed(error.to_string()),
            None => FetchPoll::Pending,
        }
    }
}

struct NativeVideoSource {
    width: u32,
    height: u32,
    frames_per_second: f64,
    frames_shown: i64,
    frames: Mutex<Receiver<Vec<u8>>>,
}

impl VideoSource for NativeVideoSource {
    /// Pulls decoded frames up to the one due at `position`, skipping
    /// frames when behind. Holds the last frame once the video ends.
    fn poll(&mut self, position: Seconds) -> Option<VideoFrame> {
        let target = (position.0 * self.frames_per_second).floor() as i64;
        let mut latest = None;
        let queue = self.frames.lock().expect("video frame queue poisoned");
        while self.frames_shown <= target {
            let Ok(frame) = queue.try_recv() else { break };
            self.frames_shown += 1;
            latest = Some(frame);
        }
        drop(queue);
        latest.map(|rgba| VideoFrame {
            width: self.width,
            height: self.height,
            rgba,
        })
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
        let mut sent_any = false;
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
            sent_any = true;
        }
        // A lap that produced nothing would rebuild forever: a broken
        // stream must wind the thread down, not busy-loop it.
        if !looping || !sent_any {
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
