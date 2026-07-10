use crate::core::platform::{AssetFetch, FetchPoll, platform};
use godot::classes::{Image, ImageTexture};
use godot::prelude::*;
use std::path::{Path, PathBuf};

/// Decodes an image's bytes by its file extension into a texture.
pub fn decode_texture(path: &Path, bytes: &[u8]) -> Option<Gd<ImageTexture>> {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut image = Image::new_gd();
    let data = PackedByteArray::from(bytes);
    let ok = match extension.as_str() {
        "png" => image.load_png_from_buffer(&data),
        "jpg" | "jpeg" => image.load_jpg_from_buffer(&data),
        _ => return None,
    };
    if ok != godot::global::Error::OK {
        return None;
    }
    ImageTexture::create_from_image(&image)
}

/// One image on its way from the platform: poll every frame until it
/// resolves. Native resolves on the first poll; the web streams over HTTP.
pub struct PendingTexture {
    path: PathBuf,
    fetch: Box<dyn AssetFetch>,
}

impl PendingTexture {
    pub fn load(path: PathBuf) -> PendingTexture {
        PendingTexture {
            fetch: platform().fetch_asset(&path),
            path,
        }
    }

    /// `None` while in flight; `Some(None)` when the file failed to load
    /// or decode; `Some(texture)` once ready.
    pub fn poll(&mut self) -> Option<Option<Gd<ImageTexture>>> {
        match self.fetch.poll() {
            FetchPoll::Pending => None,
            FetchPoll::Failed(error) => {
                godot::global::godot_warn!("image failed: {}: {error}", self.path.display());
                Some(None)
            }
            FetchPoll::Ready(bytes) => Some(decode_texture(&self.path, &bytes)),
        }
    }
}
