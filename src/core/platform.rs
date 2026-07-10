use crate::core::units::Seconds;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Everything the game needs from its host that bevy does not already
/// abstract: asset file access, user-data persistence, and video decoding.
/// The entry point installs one implementation before the app starts and
/// game code reaches it through [`platform`].
pub trait Platform: Send + Sync {
    /// The folder assets are loaded from, agreeing with how bevy's asset
    /// server resolves its default source, so paths translate between the
    /// two worlds.
    fn asset_root(&self) -> PathBuf;

    fn read_asset(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// The immediate children of an asset directory; empty when the
    /// directory does not exist.
    fn list_asset_dir(&self, dir: &Path) -> Vec<AssetEntry>;

    fn asset_exists(&self, path: &Path) -> bool;

    /// The named user-data file's contents, or `None` when it was never
    /// saved.
    fn load_user_data(&self, file_name: &str) -> io::Result<Option<String>>;

    fn save_user_data(&self, file_name: &str, json: &str) -> io::Result<()>;

    /// Where [`save_user_data`](Platform::save_user_data) puts the named
    /// file, for log messages.
    fn user_data_location(&self, file_name: &str) -> String;

    /// Opens a video for streaming; fails when the host cannot decode it.
    fn open_video(&self, path: &Path, looping: bool) -> Result<Box<dyn VideoSource>, String>;

    /// Starts playing an encoded sound. The host owns decoding and output
    /// and MUST keep playback running independently of the game loop: a
    /// stalled frame may never tear the audio.
    fn open_audio(
        &self,
        bytes: Arc<[u8]>,
        options: SoundOptions,
    ) -> Result<Box<dyn AudioChannel>, String>;
}

#[derive(Debug, Clone, Copy)]
pub struct SoundOptions {
    pub timeline: SoundTimeline,
    pub paused: bool,
    pub muted: bool,
    /// Playback gain, `0..=1`; muting silences without forgetting it.
    pub volume: f32,
}

impl Default for SoundOptions {
    fn default() -> SoundOptions {
        SoundOptions {
            timeline: SoundTimeline::WholeFile,
            paused: false,
            muted: false,
            volume: 1.0,
        }
    }
}

/// How playback traverses the sound's own timeline.
#[derive(Debug, Clone, Copy)]
pub enum SoundTimeline {
    /// The whole file, once, from the top.
    WholeFile,
    /// The whole file, once, from this position.
    From(Seconds),
    /// This `[start, start+length)` window, looping forever — a looping
    /// window never finishes.
    LoopWindow { start: Seconds, length: Seconds },
}

/// One playing sound; dropping the channel stops it.
pub trait AudioChannel: Send + Sync {
    /// Whether the sound is decoded and playback obeys this channel; until
    /// then [`position`](AudioChannel::position) stands still.
    fn is_ready(&self) -> bool;

    fn set_paused(&mut self, paused: bool);

    fn set_muted(&mut self, muted: bool);

    fn is_muted(&self) -> bool;

    /// Playback gain, `0..=1`, taking effect even while muted.
    fn set_volume(&mut self, volume: f32);

    /// Seconds into the sound's own timeline.
    fn position(&self) -> Seconds;

    /// The sound ran out; looping windows never finish.
    fn is_finished(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// One opened video, pulled frame by frame.
pub trait VideoSource: Send + Sync {
    /// The frame that should be visible `position` seconds into the video,
    /// when it differs from the last one returned. Sources pace themselves:
    /// an implementation may drop late frames to catch up, or ignore
    /// `position` entirely when the host paces playback on its own.
    fn poll(&mut self, position: Seconds) -> Option<VideoFrame>;
}

pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn install(platform: impl Platform + 'static) {
    if PLATFORM.set(Box::new(platform)).is_err() {
        panic!("a platform is already installed");
    }
}

pub fn platform() -> &'static dyn Platform {
    PLATFORM
        .get()
        .expect("no platform installed: call core::platform::install first")
        .as_ref()
}

static PLATFORM: OnceLock<Box<dyn Platform>> = OnceLock::new();
