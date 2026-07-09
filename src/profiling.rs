//! Chrome-trace capture for profiling runs: with the `profile` feature
//! compiled in, every span in the app — bevy's per-system spans included
//! — streams into a trace file that chrome://tracing and Perfetto open
//! directly. Without a capture enabled the game's log setup is untouched.

use bevy::log::BoxedLayer;
use bevy::prelude::*;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Requests a trace at `path`; call before [`app`](crate::app). Panics
/// without the `profile` feature — the spans the trace is made of only
/// exist when it is compiled in.
pub fn enable(path: PathBuf) {
    if !cfg!(feature = "profile") {
        panic!("profiling requires a build with --features profile");
    }
    CAPTURE
        .set(path)
        .expect("profiling can only be enabled once");
}

/// Ends the capture and flushes the trace file; call after the app exits.
pub fn finish() {
    #[cfg(all(feature = "profile", not(target_arch = "wasm32")))]
    drop(GUARD.lock().expect("flush guard lock poisoned").take());
}

#[cfg(all(feature = "profile", not(target_arch = "wasm32")))]
pub(crate) fn layer(_app: &mut App) -> Option<BoxedLayer> {
    let path = CAPTURE.get()?;
    let (layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
        .file(path)
        // Span fields carry the useful names (which system, which asset);
        // without them every system collapses into one anonymous span.
        .include_args(true)
        .build();
    GUARD
        .lock()
        .expect("flush guard lock poisoned")
        .replace(guard);
    Some(Box::new(layer))
}

#[cfg(not(all(feature = "profile", not(target_arch = "wasm32"))))]
pub(crate) fn layer(_app: &mut App) -> Option<BoxedLayer> {
    None
}

static CAPTURE: OnceLock<PathBuf> = OnceLock::new();

#[cfg(all(feature = "profile", not(target_arch = "wasm32")))]
static GUARD: std::sync::Mutex<Option<tracing_chrome::FlushGuard>> = std::sync::Mutex::new(None);
