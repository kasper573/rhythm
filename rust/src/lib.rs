pub mod core;
pub mod game;
pub mod launch;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod nodes;
pub mod profiling;
pub mod scenes;
#[cfg(target_arch = "wasm32")]
pub mod web;

use godot::init::InitStage;
use godot::prelude::*;

struct RhythmExtension;

#[gdextension]
unsafe impl ExtensionLibrary for RhythmExtension {
    fn on_stage_deinit(stage: InitStage) {
        if stage == InitStage::Scene {
            core::font::drop_caches();
        }
    }
}
