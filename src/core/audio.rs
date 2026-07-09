use crate::core::platform::{AudioChannel, SoundOptions, platform};
use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use std::sync::Arc;

/// An encoded sound file's raw bytes; the installed platform decodes and
/// plays them.
#[derive(Asset, TypePath, Clone)]
pub struct Sound {
    pub bytes: Arc<[u8]>,
}

/// Plays its sound while the entity lives. The
/// [`SoundChannel`] arrives once the platform has the sound ready.
#[derive(Component)]
pub struct SoundPlayer {
    pub sound: Handle<Sound>,
    pub options: SoundOptions,
}

/// The live playback handle; dropping it (with its entity) stops the
/// sound.
#[derive(Component, Deref, DerefMut)]
pub struct SoundChannel(pub Box<dyn AudioChannel>);

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Sound>()
            .init_asset_loader::<SoundLoader>()
            .add_systems(Update, start_queued_sounds);
    }
}

/// Opens a channel for every queued [`SoundPlayer`] whose bytes have
/// loaded.
fn start_queued_sounds(
    sounds: Res<Assets<Sound>>,
    queued: Query<(Entity, &SoundPlayer), Without<SoundChannel>>,
    mut commands: Commands,
) {
    for (entity, player) in &queued {
        let Some(sound) = sounds.get(&player.sound) else {
            continue;
        };
        match platform().open_audio(sound.bytes.clone(), player.options) {
            Ok(channel) => {
                commands.entity(entity).insert(SoundChannel(channel));
            }
            Err(error) => {
                warn!("sound cannot play: {error}");
                commands.entity(entity).remove::<SoundPlayer>();
            }
        }
    }
}

#[derive(Default, TypePath)]
struct SoundLoader;

impl AssetLoader for SoundLoader {
    type Asset = Sound;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Sound, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(Sound {
            bytes: bytes.into(),
        })
    }

    fn extensions(&self) -> &[&str] {
        &["ogg", "mp3", "wav"]
    }
}
