use std::path::{Path, PathBuf};

/// The folder assets are loaded from, resolved exactly like bevy's asset
/// server resolves its default source, so paths agree between the asset
/// server and our own file scanning.
pub fn asset_root() -> PathBuf {
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

/// Converts an absolute path under the asset root into the relative,
/// forward-slashed form `AssetServer::load` expects.
pub fn asset_server_path(absolute: &Path) -> Option<String> {
    let relative = absolute.strip_prefix(asset_root()).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}
