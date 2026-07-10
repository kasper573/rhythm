use crate::core::assets::asset_root;
use crate::core::audio::{self, SFX_BUS};
use crate::core::platform::platform;
use godot::classes::{AudioStream, AudioStreamPlayer, Engine};
use godot::prelude::*;
use std::collections::HashMap;
use strum::{EnumIter, IntoEnumIterator, IntoStaticStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum Sfx {
    Navigate,
    Select,
    Cancel,
    WheelMove,
    WheelSelect,
    GroupToggle,
    StartFile,
    Tick,
    Fail,
}

impl Sfx {
    pub fn asset_path(self) -> String {
        format!("sfx/{}.wav", <&str>::from(self))
    }

    /// Fires the sound on the shared player pool.
    pub fn play(self) {
        SfxPlayer::singleton().bind_mut().fire(self);
    }
}

/// The sound-effect singleton: every effect decoded once at boot, played
/// through a pool of players on the Sfx bus so overlapping cues mix.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct SfxPlayer {
    streams: HashMap<Sfx, Gd<AudioStream>>,
    pool: Vec<Gd<AudioStreamPlayer>>,
    base: Base<Node>,
}

#[godot_api]
impl SfxPlayer {
    pub fn singleton() -> Gd<SfxPlayer> {
        Engine::singleton()
            .get_singleton("SfxPlayer")
            .expect("SfxPlayer singleton is registered at boot")
            .cast()
    }

    fn fire(&mut self, sfx: Sfx) {
        let Some(stream) = self.streams.get(&sfx).cloned() else {
            return;
        };
        let mut player = match self.pool.iter().find(|player| !player.is_playing()) {
            Some(player) => player.clone(),
            None => {
                let mut player = AudioStreamPlayer::new_alloc();
                player.set_bus(SFX_BUS);
                self.base_mut().add_child(&player);
                self.pool.push(player.clone());
                player
            }
        };
        player.set_stream(&stream);
        player.play();
    }
}

#[godot_api]
impl INode for SfxPlayer {
    /// Construction stays empty — the editor instantiates registered
    /// classes while scanning, where no game state exists. The sounds
    /// load on entering the booted game's tree.
    fn init(base: Base<Node>) -> SfxPlayer {
        SfxPlayer {
            streams: HashMap::new(),
            pool: Vec::new(),
            base,
        }
    }

    fn enter_tree(&mut self) {
        self.streams = Sfx::iter()
            .filter_map(|sfx| {
                let path = asset_root().join(sfx.asset_path());
                let bytes = match platform().read_asset(&path) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        godot_warn!("sfx unavailable: {}: {error}", path.display());
                        return None;
                    }
                };
                match audio::wav_stream(&bytes) {
                    Ok(stream) => Some((sfx, stream.upcast())),
                    Err(error) => {
                        godot_warn!("sfx cannot decode: {}: {error}", path.display());
                        None
                    }
                }
            })
            .collect();
    }
}
