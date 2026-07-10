use crate::core::platform::{AssetEntry, AssetFetch, FetchPoll, Platform, VideoFrame, VideoSource};
use crate::core::units::Seconds;
use godot::classes::{HttpRequest, Node};
use godot::prelude::*;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::ffi::{CString, c_char, c_int};
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// The browser host: the asset tree is known from a deploy-generated
/// index, small text assets are prefetched into memory at boot so the game
/// can read them synchronously, larger media stream over HTTP on demand,
/// and video is decoded by the browser's own `<video>` element behind the
/// canvas.
pub struct WebPlatform {
    /// Every file under the asset root.
    files: BTreeSet<PathBuf>,
    /// The prefetched assets, keyed by their full path.
    text: HashMap<PathBuf, Vec<u8>>,
}

const ASSET_ROOT: &str = "assets";
const INDEX_FILE: &str = "index.json";

/// The deploy-generated listing of every file under the asset root,
/// written by the `serve` tool next to the assets it describes.
#[derive(serde::Deserialize)]
struct AssetIndex {
    files: Vec<String>,
}

/// Extensions the game reads synchronously via
/// [`Platform::read_asset`] — stepfiles, manifests, the small wav samples
/// mixed into generated audio — plus everything the boot loads eagerly:
/// the font, the note skins' art, and the rating images.
fn is_prefetched(file: &str) -> bool {
    let lowered = file.to_lowercase();
    if ["sm", "json", "md", "wav", "ttf", "glb"]
        .iter()
        .any(|extension| lowered.ends_with(&format!(".{extension}")))
    {
        return true;
    }
    lowered.starts_with("note_skins/") || lowered.starts_with("ratings/")
}

/// The boot prefetch: fetches the asset index, then every synchronously
/// read asset, a few requests at a time. The game's root polls it each
/// frame and installs the finished [`WebPlatform`].
pub struct WebBoot {
    host: Gd<Node>,
    index: Option<SharedResponse>,
    files: Vec<String>,
    queue: Vec<String>,
    active: Vec<(String, SharedResponse)>,
    text: HashMap<PathBuf, Vec<u8>>,
}

/// How many fetches fly at once; browsers queue past their own limit, but
/// hundreds of live HTTPRequest nodes help nobody.
const PARALLEL_FETCHES: usize = 8;

impl WebBoot {
    pub fn start(host: Gd<Node>) -> WebBoot {
        let mut boot = WebBoot {
            host,
            index: None,
            files: Vec::new(),
            queue: Vec::new(),
            active: Vec::new(),
            text: HashMap::new(),
        };
        boot.index = Some(request(&mut boot.host.clone(), INDEX_FILE));
        boot
    }

    /// Advances the prefetch; `Some` once everything is in.
    pub fn poll(&mut self) -> Option<WebPlatform> {
        if let Some(index) = &self.index {
            let Some(result) = index.borrow_mut().take() else {
                return None;
            };
            let bytes = result.unwrap_or_else(|error| {
                panic!("failed to fetch the asset index: {error}");
            });
            let index: AssetIndex = serde_json::from_slice(&bytes)
                .unwrap_or_else(|error| panic!("invalid asset index: {error}"));
            self.queue = index
                .files
                .iter()
                .filter(|file| is_prefetched(file))
                .cloned()
                .collect();
            self.files = index.files;
            self.index = None;
        }

        let mut index = 0;
        while index < self.active.len() {
            let done = self.active[index].1.borrow_mut().take();
            match done {
                None => index += 1,
                Some(result) => {
                    let (file, _) = self.active.swap_remove(index);
                    let bytes =
                        result.unwrap_or_else(|error| panic!("failed to fetch {file}: {error}"));
                    self.text.insert(Path::new(ASSET_ROOT).join(&file), bytes);
                }
            }
        }
        while self.active.len() < PARALLEL_FETCHES
            && let Some(file) = self.queue.pop()
        {
            let response = request(&mut self.host.clone(), &file);
            self.active.push((file, response));
        }

        if self.files.is_empty() || !self.active.is_empty() || !self.queue.is_empty() {
            return None;
        }
        Some(WebPlatform {
            files: self
                .files
                .iter()
                .map(|file| Path::new(ASSET_ROOT).join(file))
                .collect(),
            text: std::mem::take(&mut self.text),
        })
    }
}

type SharedResponse = Rc<RefCell<Option<Result<Vec<u8>, String>>>>;

/// Fires one HTTP GET for an asset-root-relative file through a transient
/// [`HttpRequest`] node; the shared slot resolves when it lands.
fn request(host: &mut Gd<Node>, file: &str) -> SharedResponse {
    let mut node = HttpRequest::new_alloc();
    // The browser's fetch already decompresses bodies; on gzip-encoded
    // responses (GitHub Pages) letting Godot decompress again fails.
    node.set_accept_gzip(false);
    host.add_child(&node);
    let response: SharedResponse = Rc::new(RefCell::new(None));
    let slot = Rc::clone(&response);
    let mut cleanup = node.clone();
    node.signals().request_completed().connect(
        move |result: i64, code: i64, _headers: PackedStringArray, body: PackedByteArray| {
            let outcome = if result == 0 && (200..300).contains(&code) {
                Ok(body.to_vec())
            } else {
                Err(format!("HTTP {code} (result {result})"))
            };
            slot.borrow_mut().replace(outcome);
            cleanup.queue_free();
        },
    );
    let error = node.request(&url(file));
    if error != godot::global::Error::OK {
        response
            .borrow_mut()
            .replace(Err(format!("request refused: {error:?}")));
    }
    response
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

    fn fetch_asset(&self, path: &Path) -> Box<dyn AssetFetch> {
        if let Some(bytes) = self.text.get(path) {
            return Box::new(WebFetch {
                response: Rc::new(RefCell::new(Some(Ok(bytes.clone())))),
            });
        }
        let relative = path
            .strip_prefix(ASSET_ROOT)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let mut game = crate::game::Game::singleton().upcast::<Node>();
        Box::new(WebFetch {
            response: request(&mut game, &relative),
        })
    }

    /// Videos decode in the browser's own `<video>` element, parked in the
    /// DOM behind the canvas; frames are read back through a 2d canvas.
    fn open_video(&self, path: &Path, looping: bool) -> Result<Box<dyn VideoSource>, String> {
        let url = url(&path
            .strip_prefix(ASSET_ROOT)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/"));
        let loops = if looping { "true" } else { "false" };
        let open = format!(
            r#"(function() {{
                globalThis.__rhythm_videos = globalThis.__rhythm_videos || {{ seq: 0, map: {{}} }};
                const store = globalThis.__rhythm_videos;
                const video = document.createElement('video');
                video.muted = true;
                video.loop = {loops};
                video.autoplay = true;
                video.setAttribute('playsinline', '');
                video.setAttribute('webkit-playsinline', '');
                video.style.cssText = 'position:fixed;right:100%;bottom:100%;width:1px;height:1px;opacity:0;pointer-events:none';
                video.src = "{url}";
                document.body.appendChild(video);
                video.play();
                const canvas = document.createElement('canvas');
                const context = canvas.getContext('2d', {{ willReadFrequently: true }});
                const id = ++store.seq;
                store.map[id] = {{ video: video, canvas: canvas, context: context, last: -1 }};
                return id;
            }})()"#
        );
        let id = run_script_int(&open);
        if id <= 0 {
            return Err("cannot create a video element".to_string());
        }
        Ok(Box::new(WebVideoSource { id }))
    }
}

unsafe extern "C" {
    fn emscripten_run_script(script: *const c_char);
    fn emscripten_run_script_int(script: *const c_char) -> c_int;
    fn emscripten_run_script_string(script: *const c_char) -> *const c_char;
}

/// Evaluates JS inside the engine's emscripten module scope, so scripts
/// reach both the DOM and the wasm heap views.
fn run_script(script: &str) {
    let script = CString::new(script).expect("scripts contain no NUL");
    unsafe { emscripten_run_script(script.as_ptr()) };
}

fn run_script_int(script: &str) -> i32 {
    let script = CString::new(script).expect("scripts contain no NUL");
    unsafe { emscripten_run_script_int(script.as_ptr()) }
}

fn run_script_string(script: &str) -> String {
    let script = CString::new(script).expect("scripts contain no NUL");
    let result = unsafe { emscripten_run_script_string(script.as_ptr()) };
    if result.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(result) }
        .to_string_lossy()
        .into_owned()
}

struct WebFetch {
    response: SharedResponse,
}

impl AssetFetch for WebFetch {
    fn poll(&mut self) -> FetchPoll {
        match self.response.borrow_mut().take() {
            None => FetchPoll::Pending,
            Some(Ok(bytes)) => FetchPoll::Ready(bytes),
            Some(Err(error)) => FetchPoll::Failed(error),
        }
    }
}

/// Frames are read back by drawing the playing element onto an offscreen
/// canvas, at most the viewport's width — pixels beyond that are copy cost
/// without visible gain. The browser paces playback itself, so the
/// caller's clock position is ignored; a new frame is reported whenever
/// the element's own time moves. The returned buffer carries the frame
/// size in its first eight bytes.
struct WebVideoSource {
    id: i32,
}

impl VideoSource for WebVideoSource {
    fn poll(&mut self, _position: Seconds) -> Option<VideoFrame> {
        let id = self.id;
        // First pass sizes the frame (packed as width<<16 | height, both
        // far below 65k); the second copies it into our buffer on the
        // shared heap.
        let measure = format!(
            r#"(function() {{
                const state = globalThis.__rhythm_videos && globalThis.__rhythm_videos.map[{id}];
                if (!state || state.video.readyState < 2) return 0;
                if (state.video.currentTime === state.last) return 0;
                const source_width = state.video.videoWidth;
                const source_height = state.video.videoHeight;
                if (!source_width || !source_height) return 0;
                const budget = window.innerWidth * (window.devicePixelRatio || 1);
                const scale = Math.min(1, budget / source_width);
                const width = Math.max(1, Math.round(source_width * scale));
                const height = Math.max(1, Math.round(source_height * scale));
                if (width > 65535 || height > 65535) return 0;
                state.pending = [width, height];
                return (width << 16) | height;
            }})()"#
        );
        let packed = run_script_int(&measure);
        if packed <= 0 {
            return None;
        }
        let width = (packed as u32) >> 16;
        let height = (packed as u32) & 0xffff;
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        let copy = format!(
            r#"(function() {{
                const state = globalThis.__rhythm_videos.map[{id}];
                const width = state.pending[0];
                const height = state.pending[1];
                state.last = state.video.currentTime;
                if (state.canvas.width !== width) state.canvas.width = width;
                if (state.canvas.height !== height) state.canvas.height = height;
                state.context.drawImage(state.video, 0, 0, width, height);
                const data = state.context.getImageData(0, 0, width, height);
                HEAPU8.set(data.data, {pointer});
                return 1;
            }})()"#,
            pointer = rgba.as_mut_ptr() as usize
        );
        if run_script_int(&copy) != 1 {
            return None;
        }
        Some(VideoFrame {
            width,
            height,
            rgba,
        })
    }
}

impl Drop for WebVideoSource {
    fn drop(&mut self) {
        let id = self.id;
        run_script(&format!(
            r#"(function() {{
                const store = globalThis.__rhythm_videos;
                const state = store && store.map[{id}];
                if (!state) return;
                state.video.pause();
                state.video.remove();
                delete store.map[{id}];
            }})()"#
        ));
    }
}

/// Percent-encodes a path for the browser: asset names contain characters
/// with URL meaning (`#` starts a fragment) that must not pass through
/// literally. Everything but unreserved characters and `/` is encoded.
const URL_PATH_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~')
    .remove(b'/');

/// The page's own directory URL; HTTPRequest only accepts absolute URLs.
fn page_base() -> &'static str {
    static BASE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    BASE.get_or_init(|| {
        run_script_string("location.href.slice(0, location.href.lastIndexOf('/') + 1)")
    })
}

fn url(file: &str) -> String {
    let relative = format!("{ASSET_ROOT}/{file}");
    let path = utf8_percent_encode(&relative, URL_PATH_SET);
    format!("{}{path}", page_base())
}
