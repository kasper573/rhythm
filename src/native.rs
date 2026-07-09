use crate::core::platform::{AssetEntry, Platform, VideoFrame, VideoSource};
use crate::core::units::Seconds;
use bevy::log::warn;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

/// The desktop host: assets and user data on the real filesystem, video
/// decoded in-process by `video-rs` on a worker thread.
pub struct NativePlatform;

impl Platform for NativePlatform {
    fn asset_root(&self) -> PathBuf {
        let base = if let Ok(root) = std::env::var("BEVY_ASSET_ROOT") {
            PathBuf::from(root)
        } else if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            PathBuf::from(manifest_dir)
        } else {
            std::env::current_exe()
                .expect("could not locate the executable to resolve the asset root")
                .parent()
                .expect("executable has no parent directory")
                .to_path_buf()
        };
        base.join("assets")
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

    fn load_user_data(&self, file_name: &str) -> io::Result<Option<String>> {
        let path = user_data_path(file_name);
        if !path.exists() {
            return Ok(None);
        }
        std::fs::read_to_string(&path).map(Some)
    }

    fn save_user_data(&self, file_name: &str, json: &str) -> io::Result<()> {
        let path = user_data_path(file_name);
        std::fs::create_dir_all(path.parent().expect("user data path has a parent"))?;
        std::fs::write(&path, json)
    }

    fn user_data_location(&self, file_name: &str) -> String {
        user_data_path(file_name).display().to_string()
    }

    fn open_video(&self, path: &Path, looping: bool) -> Result<Box<dyn VideoSource>, String> {
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

        Ok(Box::new(NativeVideoSource {
            width,
            height,
            frames_per_second,
            frames_shown: 0,
            frames: Mutex::new(frames),
        }))
    }
}

/// User data lives in the OS config directory under `rhythm/`.
fn user_data_path(file_name: &str) -> PathBuf {
    dirs::config_dir()
        .expect("no OS config directory available to store user data")
        .join("rhythm")
        .join(file_name)
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
