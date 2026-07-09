use crate::core::platform::platform;
use std::path::{Path, PathBuf};

/// The folder assets are loaded from, resolved by the installed platform
/// exactly like bevy's asset server resolves its default source, so paths
/// agree between the asset server and our own file scanning.
pub fn asset_root() -> PathBuf {
    platform().asset_root()
}

/// Converts an absolute path under the asset root into the relative,
/// forward-slashed form `AssetServer::load` expects.
pub fn asset_server_path(absolute: &Path) -> Option<String> {
    let relative = absolute.strip_prefix(asset_root()).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}
