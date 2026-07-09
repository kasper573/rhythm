use crate::core::platform::{
    AssetEntry, AudioChannel, Platform, SoundOptions, VideoFrame, VideoSource,
};
use crate::core::units::Seconds;
use bevy::log::warn;
use send_wrapper::SendWrapper;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use wasm_bindgen::JsCast;
use web_sys::{
    AudioBuffer, AudioBufferSourceNode, AudioContext, AudioScheduledSourceNode,
    CanvasRenderingContext2d, GainNode, HtmlCanvasElement, HtmlVideoElement,
};

/// Boots the browser build: fetches the asset index and every
/// synchronously-read text asset, then hands the platform to
/// [`run`](crate::run). Spawned from `main` when the wasm module is
/// initialized — the page only initializes it from a user gesture, which
/// unlocks audio playback.
pub fn boot() {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async {
        let platform = WebPlatform::fetch()
            .await
            .unwrap_or_else(|error| panic!("failed to fetch game assets: {error}"));
        crate::run(platform);
    });
}

/// The browser host: the asset tree is known from a deploy-generated
/// index, small text assets are prefetched into memory so the game can
/// read them synchronously, user data lives in localStorage, and video is
/// decoded by the browser's own `<video>` element. Everything else
/// streams through bevy's asset server over HTTP.
pub struct WebPlatform {
    /// Every file under the asset root.
    files: BTreeSet<PathBuf>,
    /// The prefetched text assets, keyed by their full path.
    text: HashMap<PathBuf, Vec<u8>>,
}

impl WebPlatform {
    async fn fetch() -> Result<WebPlatform, String> {
        let index = gloo_net::http::Request::get(&url(INDEX_FILE))
            .send()
            .await
            .map_err(|error| error.to_string())?
            .binary()
            .await
            .map_err(|error| error.to_string())?;
        let index: AssetIndex =
            serde_json::from_slice(&index).map_err(|error| error.to_string())?;

        let sync_read_files =
            index
                .files
                .iter()
                .filter(|file| is_sync_read(file))
                .map(|file| async move {
                    let response = gloo_net::http::Request::get(&url(file))
                        .send()
                        .await
                        .map_err(|error| format!("{file}: {error}"))?;
                    if !response.ok() {
                        return Err(format!("{file}: HTTP {}", response.status()));
                    }
                    let bytes = response
                        .binary()
                        .await
                        .map_err(|error| format!("{file}: {error}"))?;
                    Ok::<_, String>((Path::new(ASSET_ROOT).join(file), bytes))
                });
        let text = futures::future::try_join_all(sync_read_files).await?;

        Ok(WebPlatform {
            files: index
                .files
                .iter()
                .map(|file| Path::new(ASSET_ROOT).join(file))
                .collect(),
            text: text.into_iter().collect(),
        })
    }
}

impl Platform for WebPlatform {
    fn asset_root(&self) -> PathBuf {
        PathBuf::from(ASSET_ROOT)
    }

    fn read_asset(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.text.get(path).cloned().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} is not a prefetched asset", path.display()),
            )
        })
    }

    fn list_asset_dir(&self, dir: &Path) -> Vec<AssetEntry> {
        let mut children: BTreeSet<(PathBuf, bool)> = BTreeSet::new();
        for file in &self.files {
            let Ok(rest) = file.strip_prefix(dir) else {
                continue;
            };
            let mut components = rest.components();
            let Some(first) = components.next() else {
                continue;
            };
            let is_dir = components.next().is_some();
            children.insert((dir.join(first), is_dir));
        }
        children
            .into_iter()
            .map(|(path, is_dir)| AssetEntry { path, is_dir })
            .collect()
    }

    fn asset_exists(&self, path: &Path) -> bool {
        self.files.contains(path) || self.files.iter().any(|file| file.starts_with(path))
    }

    fn load_user_data(&self, file_name: &str) -> io::Result<Option<String>> {
        local_storage()?
            .get_item(&user_data_key(file_name))
            .map_err(|_| io::Error::other("localStorage read failed"))
    }

    fn save_user_data(&self, file_name: &str, json: &str) -> io::Result<()> {
        local_storage()?
            .set_item(&user_data_key(file_name), json)
            .map_err(|_| io::Error::other("localStorage write failed (quota?)"))
    }

    fn user_data_location(&self, file_name: &str) -> String {
        format!("localStorage {}", user_data_key(file_name))
    }

    fn open_video(&self, path: &Path, looping: bool) -> Result<Box<dyn VideoSource>, String> {
        let document = web_sys::window()
            .and_then(|window| window.document())
            .ok_or("no document")?;
        let video: HtmlVideoElement = document
            .create_element("video")
            .map_err(|_| "cannot create a video element")?
            .dyn_into()
            .map_err(|_| "not a video element")?;
        // Rejecting unplayable containers here lets callers fall back to
        // their static background, like the desktop decoder does.
        let mime = video_mime(path)?;
        if video.can_play_type(mime).is_empty() {
            return Err(format!("this browser cannot play {mime}"));
        }
        video.set_muted(true);
        video.set_loop(looping);
        video.set_autoplay(true);
        // Without inline playback, iOS Safari hijacks play() into its
        // fullscreen native player instead of feeding our texture.
        let _ = video.set_attribute("playsinline", "");
        let _ = video.set_attribute("webkit-playsinline", "");
        // iOS only decodes videos that live in the DOM and are not
        // display:none, so park the element offscreen; the poll below
        // copies its frames into the game's texture.
        let _ = video.set_attribute(
            "style",
            "position:fixed;right:100%;bottom:100%;width:1px;height:1px;opacity:0;pointer-events:none",
        );
        video.set_src(&encode_path(&path.to_string_lossy().replace('\\', "/")));
        document
            .body()
            .ok_or("no body to attach the video to")?
            .append_child(&video)
            .map_err(|_| "cannot attach the video element")?;
        let _ = video.play();

        let canvas: HtmlCanvasElement = document
            .create_element("canvas")
            .map_err(|_| "cannot create a canvas")?
            .dyn_into()
            .map_err(|_| "not a canvas element")?;
        // Every frame is read back to the CPU; without this hint each
        // read forces a GPU sync stall long enough to starve the audio.
        let options = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&options, &"willReadFrequently".into(), &true.into());
        let context: CanvasRenderingContext2d = canvas
            .get_context_with_context_options("2d", &options)
            .ok()
            .flatten()
            .ok_or("no 2d canvas context")?
            .dyn_into()
            .map_err(|_| "not a 2d context")?;

        Ok(Box::new(WebVideoSource {
            inner: SendWrapper::new(WebVideo {
                video,
                canvas,
                context,
            }),
            last_time: -1.0,
        }))
    }

    /// Sounds are handed to the browser's own audio engine: decoding runs
    /// off the main thread and scheduled nodes keep playing on the
    /// browser's audio thread no matter how long a game frame stalls.
    /// Loop windows map directly onto `loopStart`/`loopEnd`.
    fn open_audio(
        &self,
        bytes: Arc<[u8]>,
        options: SoundOptions,
    ) -> Result<Box<dyn AudioChannel>, String> {
        let context = audio_context()?;
        let gain: GainNode = context
            .create_gain()
            .map_err(|_| "cannot create a gain node")?;
        gain.gain().set_value(if options.muted { 0.0 } else { 1.0 });
        gain.connect_with_audio_node(&context.destination())
            .map_err(|_| "cannot reach the audio output")?;

        let window = options
            .window
            .map(|(start, length)| (start.0.max(0.0), length.0));
        let state = Rc::new(RefCell::new(WebAudio {
            context: context.clone(),
            gain,
            buffer: None,
            node: None,
            window,
            paused: options.paused,
            muted: options.muted,
            offset: window.map(|(start, _)| start).unwrap_or(0.0),
            started_at: 0.0,
            failed: false,
            dropped: false,
        }));

        // The browser decodes in the background; playback starts (unless
        // paused) the moment the buffer is ready.
        let encoded = js_sys::Uint8Array::from(bytes.as_ref()).buffer();
        let decoding = context
            .decode_audio_data(&encoded)
            .map_err(|_| "the browser refused to decode this sound")?;
        wasm_bindgen_futures::spawn_local({
            let state = Rc::clone(&state);
            async move {
                let decoded = wasm_bindgen_futures::JsFuture::from(decoding).await;
                let mut state = state.borrow_mut();
                if state.dropped {
                    return;
                }
                match decoded {
                    Ok(buffer) => {
                        state.buffer = Some(buffer.unchecked_into::<AudioBuffer>());
                        if !state.paused {
                            state.start_node();
                        }
                    }
                    Err(error) => {
                        warn!("sound decode failed: {error:?}");
                        state.failed = true;
                    }
                }
            }
        });

        Ok(Box::new(WebChannel {
            state: SendWrapper::new(state),
        }))
    }
}

const ASSET_ROOT: &str = "assets";
const INDEX_FILE: &str = "index.json";

/// The page's width in physical pixels.
fn viewport_width() -> Option<f64> {
    let window = web_sys::window()?;
    Some(window.inner_width().ok()?.as_f64()? * window.device_pixel_ratio())
}

fn url(file: &str) -> String {
    encode_path(&format!("{ASSET_ROOT}/{file}"))
}

/// Percent-encodes a path for the browser: asset names contain characters
/// with URL meaning (`#` starts a fragment) that must not pass through
/// literally.
fn encode_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// The deploy-generated listing of every file under the asset root,
/// written by the `serve` tool next to the assets it describes.
#[derive(serde::Deserialize)]
struct AssetIndex {
    files: Vec<String>,
}

/// Extensions the game reads synchronously via
/// [`Platform::read_asset`] — stepfiles, manifests, and the small wav
/// samples mixed into generated audio. Everything else streams through
/// bevy's asset server.
fn is_sync_read(file: &str) -> bool {
    let lowered = file.to_lowercase();
    ["sm", "json", "md", "wav"]
        .iter()
        .any(|extension| lowered.ends_with(&format!(".{extension}")))
}

fn local_storage() -> io::Result<web_sys::Storage> {
    web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .ok_or_else(|| io::Error::other("localStorage unavailable"))
}

fn user_data_key(file_name: &str) -> String {
    format!("rhythm/{file_name}")
}

fn video_mime(path: &Path) -> Result<&'static str, String> {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "mp4" => Ok("video/mp4"),
        "webm" => Ok("video/webm"),
        "ogv" => Ok("video/ogg"),
        "avi" => Ok("video/x-msvideo"),
        "mpg" | "mpeg" => Ok("video/mpeg"),
        other => Err(format!("unknown video extension {other:?}")),
    }
}

/// The one AudioContext, created on first use — always after the page's
/// start gesture, so the browser lets it run.
fn audio_context() -> Result<AudioContext, String> {
    static CONTEXT: OnceLock<SendWrapper<Option<AudioContext>>> = OnceLock::new();
    let context: &Option<AudioContext> =
        CONTEXT.get_or_init(|| SendWrapper::new(AudioContext::new().ok()));
    context
        .clone()
        .ok_or_else(|| "no audio context".to_string())
}

/// One sound in the browser's audio graph. The active node plays
/// autonomously on the browser's audio thread; this state only creates,
/// stops, and observes it. Positions derive from the context clock, so
/// they track the speakers.
struct WebAudio {
    context: AudioContext,
    gain: GainNode,
    buffer: Option<AudioBuffer>,
    node: Option<AudioBufferSourceNode>,
    /// `(start, length)` of a loop window; non-positive lengths play the
    /// file whole from `start`.
    window: Option<(f64, f64)>,
    paused: bool,
    muted: bool,
    /// The sound-timeline position playback (re)started at, and when.
    offset: f64,
    started_at: f64,
    failed: bool,
    dropped: bool,
}

impl WebAudio {
    fn start_node(&mut self) {
        let Some(buffer) = &self.buffer else {
            return;
        };
        let Ok(node) = self.context.create_buffer_source() else {
            warn!("cannot create an audio source node");
            self.failed = true;
            return;
        };
        node.set_buffer(Some(buffer));
        if let Some((start, length)) = self.window
            && length > 0.0
        {
            node.set_loop(true);
            node.set_loop_start(start);
            node.set_loop_end((start + length).min(buffer.duration()));
        }
        let _ = node.connect_with_audio_node(&self.gain);
        let _ = node.start_with_when_and_grain_offset(0.0, self.offset);
        self.started_at = self.context.current_time();
        self.node = Some(node);
    }

    fn stop_node(&mut self) {
        if let Some(node) = self.node.take() {
            let scheduled: &AudioScheduledSourceNode = node.as_ref();
            let _ = scheduled.stop();
            let _ = node.disconnect();
        }
    }

    fn position(&self) -> f64 {
        if self.node.is_none() {
            return self.offset;
        }
        let raw = self.offset + (self.context.current_time() - self.started_at);
        match self.window {
            Some((start, length)) if length > 0.0 => start + (raw - start).rem_euclid(length),
            _ => match &self.buffer {
                Some(buffer) => raw.min(buffer.duration()),
                None => raw,
            },
        }
    }

    fn looping(&self) -> bool {
        matches!(self.window, Some((_, length)) if length > 0.0)
    }
}

struct WebChannel {
    state: SendWrapper<Rc<RefCell<WebAudio>>>,
}

impl AudioChannel for WebChannel {
    fn is_ready(&self) -> bool {
        self.state.borrow().buffer.is_some()
    }

    fn set_paused(&mut self, paused: bool) {
        let mut state = self.state.borrow_mut();
        if state.paused == paused {
            return;
        }
        state.paused = paused;
        if paused {
            state.offset = state.position();
            state.stop_node();
        } else {
            state.start_node();
        }
    }

    fn set_muted(&mut self, muted: bool) {
        let mut state = self.state.borrow_mut();
        state.muted = muted;
        state.gain.gain().set_value(if muted { 0.0 } else { 1.0 });
    }

    fn is_muted(&self) -> bool {
        self.state.borrow().muted
    }

    fn position(&self) -> Seconds {
        Seconds(self.state.borrow().position())
    }

    fn is_finished(&self) -> bool {
        let state = self.state.borrow();
        if state.failed {
            return true;
        }
        if state.looping() {
            return false;
        }
        match &state.buffer {
            Some(buffer) => !state.paused && state.position() >= buffer.duration() - 0.001,
            None => false,
        }
    }
}

impl Drop for WebChannel {
    fn drop(&mut self) {
        let mut state = self.state.borrow_mut();
        state.dropped = true;
        state.stop_node();
        let _ = state.gain.disconnect();
    }
}

struct WebVideo {
    video: HtmlVideoElement,
    canvas: HtmlCanvasElement,
    context: CanvasRenderingContext2d,
}

/// Frames are read back by drawing the playing element onto an offscreen
/// canvas. The browser paces playback itself, so the caller's clock
/// position is ignored; a new frame is reported whenever the element's
/// own time moves.
struct WebVideoSource {
    inner: SendWrapper<WebVideo>,
    last_time: f64,
}

impl Drop for WebVideoSource {
    fn drop(&mut self) {
        let _ = self.inner.video.pause();
        self.inner.video.remove();
    }
}

impl VideoSource for WebVideoSource {
    fn poll(&mut self, _position: Seconds) -> Option<VideoFrame> {
        const HAVE_CURRENT_DATA: u16 = 2;
        let WebVideo {
            video,
            canvas,
            context,
        } = &*self.inner;
        if video.ready_state() < HAVE_CURRENT_DATA {
            return None;
        }
        let time = video.current_time();
        if time == self.last_time {
            return None;
        }
        self.last_time = time;
        let (source_width, source_height) = (video.video_width(), video.video_height());
        if source_width == 0 || source_height == 0 {
            return None;
        }
        // Frames are read back at the size the viewport can actually
        // show; pixels beyond that are copy cost without visible gain.
        let scale =
            (viewport_width().unwrap_or(source_width as f64) / source_width as f64).min(1.0);
        let width = (source_width as f64 * scale).round() as u32;
        let height = (source_height as f64 * scale).round() as u32;
        if canvas.width() != width {
            canvas.set_width(width);
        }
        if canvas.height() != height {
            canvas.set_height(height);
        }
        context
            .draw_image_with_html_video_element_and_dw_and_dh(
                video,
                0.0,
                0.0,
                width as f64,
                height as f64,
            )
            .ok()?;
        let data = context
            .get_image_data(0.0, 0.0, width as f64, height as f64)
            .ok()?;
        Some(VideoFrame {
            width,
            height,
            rgba: data.data().0,
        })
    }
}
