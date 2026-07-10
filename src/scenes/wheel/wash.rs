use super::{WHEEL_EASE_RATE, Wheel, WheelEntry};
use crate::core::library::StepfileLibrary;
use crate::core::units::Seconds;
use crate::prefabs::media_cover::{MediaCoverPrefabOptions, MediaPace, media_cover_prefab};
use crate::scenes::GameScene;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::path::PathBuf;

/// One layer of the scene background wash: the active row's background —
/// image or looping video — over the green backdrop, as a media cover.
/// Changing rows cross-fades: the incoming layer waits invisible until its
/// image has actually loaded, then retires every older layer while it
/// eases in — so the old background always fades under a renderable new
/// one, never against a gap that a late-loading image would pop into.
#[derive(Component)]
pub(super) struct SceneBackground {
    /// The opacity this layer eases toward; reaching zero retires it.
    target: f32,
    /// Spawn order; the newest layer leads and retires the older ones.
    sequence: u32,
    /// The file this layer shows, the identity that keeps a re-selected
    /// background from restarting (videos get a fresh handle per spawn,
    /// so handles cannot be the identity).
    source: PathBuf,
}

const BACKGROUND_OPACITY: f32 = 0.25;

#[derive(SystemParam)]
pub(super) struct BackgroundAssets<'w> {
    asset_server: Res<'w, AssetServer>,
    images: ResMut<'w, Assets<Image>>,
    time: Res<'w, Time>,
}

pub(super) fn refresh_scene_background(
    wheel: Res<Wheel>,
    library: Res<StepfileLibrary>,
    mut assets: BackgroundAssets,
    mut layers: Query<(&mut SceneBackground, &Sprite)>,
    mut commands: Commands,
    mut layer_count: Local<u32>,
) {
    if !wheel.just_settled {
        return;
    }
    // Rows without a background of their own fall back to the default
    // BGM's, so the scene always has one to show.
    let path = match wheel.entries.get(wheel.active) {
        Some(WheelEntry::Stepfile { id }) => library.stepfile(*id).background_path(),
        _ => None,
    }
    .or_else(|| library.default_bgm.background_path());
    let Some(path) = path else {
        // Nothing to show at all: fade everything out.
        for (mut layer, _) in &mut layers {
            layer.target = 0.0;
        }
        return;
    };
    let already_shown = layers
        .iter()
        .any(|(layer, _)| layer.target > 0.0 && layer.source == path);
    if already_shown {
        return;
    }
    // Newer layers draw above the ones fading out; the small cycling bump
    // stays well below everything else in the scene.
    let z = 0.5 + ((*layer_count + 1) % 100) as f32 * 0.002;
    let cover = media_cover_prefab(
        MediaCoverPrefabOptions {
            path: path.clone(),
            color: Color::srgba(1.0, 1.0, 1.0, 0.0),
            z,
            start: Seconds(assets.time.elapsed_secs_f64()),
            looping: true,
            pace: MediaPace::Wall,
        },
        &mut commands,
        &assets.asset_server,
        &mut assets.images,
    );
    let Some(cover) = cover else { return };
    *layer_count += 1;
    commands.entity(cover).insert((
        SceneBackground {
            target: BACKGROUND_OPACITY,
            sequence: *layer_count,
            source: path,
        },
        DespawnOnExit(GameScene::Wheel),
    ));
}

/// Eases every background layer toward its target opacity at the wheel's
/// settle rate and retires the fully faded-out ones. Layers whose image is
/// still loading hold at zero: only a loaded layer may lead, and only the
/// leader retires the layers beneath it.
pub(super) fn fade_scene_background(
    time: Res<Time>,
    images: Res<Assets<Image>>,
    mut layers: Query<(Entity, &mut SceneBackground, &mut Sprite)>,
    mut commands: Commands,
) {
    let leader = layers
        .iter()
        .filter(|(_, layer, sprite)| layer.target > 0.0 && images.contains(&sprite.image))
        .max_by_key(|(_, layer, _)| layer.sequence)
        .map(|(entity, layer, _)| (entity, layer.sequence));
    if let Some((leader, leader_sequence)) = leader {
        for (entity, mut layer, _) in &mut layers {
            if entity != leader && layer.sequence < leader_sequence && layer.target > 0.0 {
                layer.target = 0.0;
            }
        }
    }

    let ease = 1.0 - (-WHEEL_EASE_RATE * time.delta_secs()).exp();
    for (entity, layer, mut sprite) in &mut layers {
        if layer.target > 0.0 && !images.contains(&sprite.image) {
            continue;
        }
        let alpha = sprite.color.alpha();
        let mut next = alpha + (layer.target - alpha) * ease;
        if (next - layer.target).abs() < 0.002 {
            next = layer.target;
        }
        if next != alpha {
            sprite.color.set_alpha(next);
        }
        if layer.target <= 0.0 && next <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}
