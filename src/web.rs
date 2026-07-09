use crate::core::platform::{AssetEntry, Platform, VideoFrame, VideoSource};
use crate::core::units::Seconds;
use send_wrapper::SendWrapper;
use std::collections::{BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlVideoElement};

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
        video.set_src(&encode_path(&path.to_string_lossy().replace('\\', "/")));
        let _ = video.play();

        let canvas: HtmlCanvasElement = document
            .create_element("canvas")
            .map_err(|_| "cannot create a canvas")?
            .dyn_into()
            .map_err(|_| "not a canvas element")?;
        let context: CanvasRenderingContext2d = canvas
            .get_context("2d")
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
}

const ASSET_ROOT: &str = "assets";
const INDEX_FILE: &str = "index.json";

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
        let (width, height) = (video.video_width(), video.video_height());
        if width == 0 || height == 0 {
            return None;
        }
        if canvas.width() != width {
            canvas.set_width(width);
        }
        if canvas.height() != height {
            canvas.set_height(height);
        }
        context
            .draw_image_with_html_video_element(video, 0.0, 0.0)
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
