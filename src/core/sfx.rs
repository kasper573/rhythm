use bevy::audio::PlaybackMode;
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
struct SfxLibrary(HashMap<Sfx, Handle<AudioSource>>);

fn load_sfx(mut commands: Commands, asset_server: Res<AssetServer>) {
    let handles = Sfx::iter()
        .map(|sfx| (sfx, asset_server.load(sfx.asset_path())))
        .collect();
    commands.insert_resource(SfxLibrary(handles));
}

fn play_requested_sfx(
    mut requests: MessageReader<PlaySfx>,
    library: Res<SfxLibrary>,
    mut commands: Commands,
) {
    for PlaySfx(sfx) in requests.read() {
        let source = library.0[sfx].clone();
        commands.spawn_scene(bsn! {
            AudioPlayer({source})
            PlaybackSettings { mode: {PlaybackMode::Despawn} }
        });
    }
}
