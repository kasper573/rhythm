pub mod core;
pub mod dev;
pub mod game;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod nodes;
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
