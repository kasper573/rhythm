use crate::core::audio::Sound;
use crate::core::platform::{AudioChannel, SoundOptions, platform};
use bevy::prelude::*;
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
}

#[derive(Message)]
pub struct PlaySfx(pub Sfx);

pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlaySfx>()
            .add_systems(Startup, load_sfx)
            .add_systems(Update, play_requested_sfx);
    }
}

#[derive(Resource)]
struct SfxLibrary(HashMap<Sfx, Handle<Sound>>);

fn load_sfx(mut commands: Commands, asset_server: Res<AssetServer>) {
    let handles = Sfx::iter()
        .map(|sfx| (sfx, asset_server.load(sfx.asset_path())))
        .collect();
    commands.insert_resource(SfxLibrary(handles));
}

/// Fired sounds live in the local pool until they finish, so their
/// channels stay alive for as long as they play.
fn play_requested_sfx(
    mut requests: MessageReader<PlaySfx>,
    library: Res<SfxLibrary>,
    sounds: Res<Assets<Sound>>,
    mut live: Local<Vec<Box<dyn AudioChannel>>>,
) {
    live.retain(|channel| !channel.is_finished());
    for PlaySfx(sfx) in requests.read() {
        let Some(sound) = sounds.get(&library.0[sfx]) else {
            continue;
        };
        match platform().open_audio(sound.bytes.clone(), SoundOptions::default()) {
            Ok(channel) => live.push(channel),
            Err(error) => warn!("sfx cannot play: {error}"),
        }
    }
}
