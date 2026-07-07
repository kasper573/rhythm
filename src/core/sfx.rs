use bevy::prelude::*;
use std::collections::HashMap;

/// Every unique sound effect in the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sfx {
    /// A: menu next/previous
    Navigate,
    /// B: menu select
    Select,
    /// C: cancel / go back
    Cancel,
    /// D: stepfile wheel next/previous
    WheelMove,
    /// E: stepfile wheel select
    WheelSelect,
    /// F: group collapse toggle
    GroupToggle,
    /// G: start a file
    StartFile,
    /// H: metronome tick (also mixed into the pre-rendered tick track)
    Tick,
}

impl Sfx {
    pub const ALL: [Sfx; 8] = [
        Sfx::Navigate,
        Sfx::Select,
        Sfx::Cancel,
        Sfx::WheelMove,
        Sfx::WheelSelect,
        Sfx::GroupToggle,
        Sfx::StartFile,
        Sfx::Tick,
    ];

    /// Path relative to the asset root.
    pub fn asset_path(self) -> &'static str {
        match self {
            Sfx::Navigate => "sfx/navigate.wav",
            Sfx::Select => "sfx/select.wav",
            Sfx::Cancel => "sfx/cancel.wav",
            Sfx::WheelMove => "sfx/wheel_move.wav",
            Sfx::WheelSelect => "sfx/wheel_select.wav",
            Sfx::GroupToggle => "sfx/group_toggle.wav",
            Sfx::StartFile => "sfx/start_file.wav",
            Sfx::Tick => "sfx/tick.wav",
        }
    }
}

/// Write this message from anywhere to play a one-shot sound effect.
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
    let handles = Sfx::ALL
        .into_iter()
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
        commands.spawn((
            AudioPlayer(library.0[sfx].clone()),
            PlaybackSettings::DESPAWN,
        ));
    }
}
