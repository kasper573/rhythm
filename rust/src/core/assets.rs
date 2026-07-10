use crate::core::platform::platform;
use std::path::{Path, PathBuf};

/// The folder assets are loaded from, resolved by the installed platform.
pub fn asset_root() -> PathBuf {
    platform().asset_root()
}

/// Converts an absolute path under the asset root into its relative,
/// forward-slashed form — the shape web URLs and log messages want.
pub fn asset_relative_path(absolute: &Path) -> Option<String> {
    let relative = absolute.strip_prefix(asset_root()).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}
