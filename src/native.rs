use crate::core::platform::{
    AssetEntry, AudioChannel, Platform, SoundOptions, VideoFrame, VideoSource,
};
use crate::core::units::Seconds;
use bevy::log::warn;
use rodio::Source;
use std::io;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

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

    /// Sounds mix on rodio's dedicated audio thread; the game loop only
    /// issues commands, so a stalled frame never tears playback. Loop
    /// windows are entered and looped with decoder seeks — instant jumps,
    /// where decoding-and-discarding up to a window would stall the frame
    /// that starts the music.
    fn open_audio(
        &self,
        bytes: Arc<[u8]>,
        options: SoundOptions,
    ) -> Result<Box<dyn AudioChannel>, String> {
        let mixer = output_mixer().ok_or("no audio device")?;
        let sink = rodio::Player::connect_new(mixer);
        if options.paused {
            sink.pause();
        }
        if options.muted {
            sink.set_volume(0.0);
        }
        let window = match options.window {
            Some((start, length)) => {
                sink.append(WindowedMusic::open(bytes, start, length));
                Some((start.0.max(0.0), length.0))
            }
            None => {
                let decoder =
                    rodio::Decoder::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
                sink.append(decoder);
                None
            }
        };
        Ok(Box::new(NativeChannel {
            sink,
            window,
            muted: options.muted,
        }))
    }
}

/// The one output stream, opened on first use and kept for the process's
/// lifetime.
fn output_mixer() -> Option<&'static rodio::mixer::Mixer> {
    static OUTPUT: OnceLock<Option<rodio::MixerDeviceSink>> = OnceLock::new();
    OUTPUT
        .get_or_init(|| {
            rodio::DeviceSinkBuilder::open_default_sink()
                .map(|mut sink| {
                    sink.log_on_drop(false);
                    sink
                })
                .inspect_err(|error| warn!("no audio device: {error}"))
                .ok()
        })
        .as_ref()
        .map(|sink| sink.mixer())
}

struct NativeChannel {
    sink: rodio::Player,
    /// `(start, length)` when playing a loop window, for folding the
    /// sink's monotonic position back into the sound's timeline.
    window: Option<(f64, f64)>,
    muted: bool,
}

impl AudioChannel for NativeChannel {
    fn is_ready(&self) -> bool {
        true
    }

    fn set_paused(&mut self, paused: bool) {
        if paused {
            self.sink.pause();
        } else {
            self.sink.play();
        }
    }

    fn set_muted(&mut self, muted: bool) {
        self.sink.set_volume(if muted { 0.0 } else { 1.0 });
        self.muted = muted;
    }

    fn is_muted(&self) -> bool {
        self.muted
    }

    fn position(&self) -> Seconds {
        let raw = self.sink.get_pos().as_secs_f64();
        match self.window {
            Some((start, length)) if length > 0.0 => Seconds(start + raw.rem_euclid(length)),
            Some((start, _)) => Seconds(start + raw),
            None => Seconds(raw),
        }
    }

    fn is_finished(&self) -> bool {
        self.sink.empty()
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

/// Music playing its preview sample window: the decoder is seeked to the
/// window start when opened and seeked back whenever the window runs out.
struct WindowedMusic {
    decoder: rodio::Decoder<Cursor<Arc<[u8]>>>,
    start: Duration,
    /// The window length, while looping it stays possible.
    window: Option<Duration>,
    /// Samples yielded since the window start.
    played: u64,
}

impl WindowedMusic {
    fn open(bytes: Arc<[u8]>, start: Seconds, length: Seconds) -> WindowedMusic {
        let mut decoder = rodio::Decoder::new(Cursor::new(bytes))
            .expect("music bytes already decoded once as an AudioSource");
        let start = Duration::from_secs_f64(start.0.max(0.0));
        let window = match decoder.try_seek(start) {
            Ok(()) => (length.0 > 0.0).then(|| Duration::from_secs_f64(length.0)),
            Err(error) => {
                warn!("music cannot seek, playing it whole: {error:?}");
                None
            }
        };
        WindowedMusic {
            decoder,
            start,
            window,
            played: 0,
        }
    }

    fn window_samples(&self, window: Duration) -> u64 {
        let per_second =
            self.decoder.sample_rate().get() as u64 * self.decoder.channels().get() as u64;
        (window.as_secs_f64() * per_second as f64) as u64
    }

    fn rewind(&mut self) -> bool {
        self.played = 0;
        match self.decoder.try_seek(self.start) {
            Ok(()) => true,
            Err(error) => {
                warn!("music stopped looping its window: {error:?}");
                self.window = None;
                false
            }
        }
    }
}

impl Iterator for WindowedMusic {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<rodio::Sample> {
        if let Some(window) = self.window
            && self.played >= self.window_samples(window)
        {
            self.rewind();
        }
        match self.decoder.next() {
            Some(sample) => {
                self.played += 1;
                Some(sample)
            }
            // The file ran out inside the window; wrap early.
            None if self.window.is_some() && self.rewind() => self.decoder.next(),
            None => None,
        }
    }
}

impl Source for WindowedMusic {
    fn current_span_len(&self) -> Option<usize> {
        self.decoder.current_span_len()
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.decoder.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.decoder.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        None
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
