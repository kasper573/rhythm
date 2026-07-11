//! Chrome-trace capture for profiling runs: with the `profile` feature
//! compiled in, the instrumented spans around the game's frame phases
//! stream into a trace file that chrome://tracing and Perfetto open
//! directly. Without a capture enabled the game's logging is untouched.

use std::path::PathBuf;

/// Requests a trace at `path`; call before the game boots. Panics without
/// the `profile` feature — the spans the trace is made of only exist when
/// it is compiled in.
#[cfg(feature = "profile")]
pub fn enable(path: PathBuf) {
    use tracing_subscriber::layer::SubscriberExt;
    let (layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
        .file(path.clone())
        // Span fields carry the useful names (which scene, which phase);
        // without them every span collapses into one anonymous entry.
        .include_args(true)
        .build();
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(layer))
        .expect("profiling can only be enabled once");
    GUARD
        .lock()
        .expect("flush guard lock poisoned")
        .replace(guard);
    println!("{}", serde_json::json!({ "trace_file": path }));
}

#[cfg(not(feature = "profile"))]
pub fn enable(_path: PathBuf) {
    panic!("profiling requires a build with --features profile");
}

/// Ends the capture and flushes the trace file; call before the game quits.
pub fn finish() {
    #[cfg(feature = "profile")]
    drop(GUARD.lock().expect("flush guard lock poisoned").take());
}

#[cfg(feature = "profile")]
static GUARD: std::sync::Mutex<Option<tracing_chrome::FlushGuard>> = std::sync::Mutex::new(None);
