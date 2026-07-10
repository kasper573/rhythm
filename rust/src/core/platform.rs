use crate::core::units::Seconds;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Everything the game needs from its host that Godot does not already
/// abstract: asset file access and video decoding (Godot only decodes
/// Theora, the stepfile world ships mp4/avi/mpg). The entry point installs
/// one implementation before the game boots and game code reaches it
/// through [`platform`].
pub trait Platform: Send + Sync {
    /// The folder assets are loaded from.
    fn asset_root(&self) -> PathBuf;

    /// Synchronously readable assets: everything on native, only the
    /// boot-prefetched text assets on the web.
    fn read_asset(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// The immediate children of an asset directory; empty when the
    /// directory does not exist.
    fn list_asset_dir(&self, dir: &Path) -> Vec<AssetEntry>;

    fn asset_exists(&self, path: &Path) -> bool;

    /// Starts loading an asset's bytes; the caller polls the returned
    /// fetch every frame until it resolves. Native resolves immediately;
    /// the web streams over HTTP.
    fn fetch_asset(&self, path: &Path) -> Box<dyn AssetFetch>;

    /// Opens a video for streaming; fails when the host cannot decode it.
    fn open_video(&self, path: &Path, looping: bool) -> Result<Box<dyn VideoSource>, String>;
}

/// One in-flight asset read; poll it every frame until it resolves.
pub trait AssetFetch {
    fn poll(&mut self) -> FetchPoll;
}

pub enum FetchPoll {
    Pending,
    Ready(Vec<u8>),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// One opened video, pulled frame by frame.
pub trait VideoSource {
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
