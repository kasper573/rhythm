use crate::core::assets::asset_root;
use crate::core::settings::Settings;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use serde::Deserialize;

/// The loaded note skin: one sprite sheet with atlas indices for taps,
/// receptors, hold heads/caps, mines, and explosions, plus standalone
/// vertically-tileable hold body images. Loaded from
/// `assets/note_skins/<name>/manifest.json`, where `<name>` comes from the
/// settings' stepfile options.
#[derive(Resource)]
pub struct ActiveNoteSkin {
    /// Folder name of the skin, as referenced by the settings.
    pub name: String,
    pub sheet: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    /// `(quant, base atlas index)` per tap row; frames follow the base.
    tap_rows: Vec<(u32, usize)>,
    pub tap_frames: usize,
    pub tap_beats_per_cycle: f64,
    pub receptor_frames: [usize; 2],
    /// Fraction of a beat the first receptor frame is shown.
    pub receptor_beat_split: f64,
    /// One entry per tap row for note-colored skins, or a single shared one.
    hold_head_inactive: Vec<usize>,
    hold_head_active: Vec<usize>,
    pub hold_cap_inactive: usize,
    pub hold_cap_active: usize,
    pub mine: usize,
    pub mine_spin_beats: f64,
    pub tap_explosion: usize,
    pub mine_explosion: usize,
    pub hold_body_inactive: Handle<Image>,
    pub hold_body_active: Handle<Image>,
    pub hold_body_size: Vec2,
    /// The body ends this many texture pixels above the tail center, where
    /// the cap takes over.
    pub hold_body_stop_above_tail: f32,
    pub hold_cap_size: Vec2,
    /// Brightness of dropped (NG) hold parts, per the skin.
    pub dropped_brightness: f32,
}

impl ActiveNoteSkin {
    /// The skin row for a quant; unknown quants use the last (finest) row.
    pub fn quant_row(&self, quant: u32) -> usize {
        self.tap_rows
            .iter()
            .position(|(row_quant, _)| *row_quant == quant)
            .unwrap_or(self.tap_rows.len() - 1)
    }

    /// Base atlas index of the tap animation for a skin row.
    pub fn tap_base(&self, row: usize) -> usize {
        self.tap_rows[row.min(self.tap_rows.len() - 1)].1
    }

    /// Hold head sprite for a skin row; skins with a single shared head
    /// ignore the row.
    pub fn hold_head(&self, row: usize, active: bool) -> usize {
        let heads = if active {
            &self.hold_head_active
        } else {
            &self.hold_head_inactive
        };
        heads[row.min(heads.len() - 1)]
    }
}

/// Every note skin found under `assets/note_skins`, for the player options
/// scene to offer.
#[derive(Resource)]
pub struct NoteSkinLibrary {
    pub skins: Vec<NoteSkinEntry>,
}

pub struct NoteSkinEntry {
    /// Folder name, as stored in the settings.
    pub name: String,
    pub display_name: String,
}

pub struct NoteSkinPlugin;

impl Plugin for NoteSkinPlugin {
    fn build(&self, app: &mut App) {
        let name = app
            .world()
            .resource::<Settings>()
            .stepfile
            .note_skin
            .clone();
        let skin = app.world_mut().resource_scope(
            |world, mut layouts: Mut<Assets<TextureAtlasLayout>>| {
                load_note_skin(world.resource::<AssetServer>(), &mut layouts, &name)
            },
        );
        app.insert_resource(skin)
            .insert_resource(scan_note_skins())
            .add_systems(Update, reload_changed_skin);
    }
}

/// Panics on a missing or invalid skin: the requested skin must exist.
pub fn load_note_skin(
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    name: &str,
) -> ActiveNoteSkin {
    let folder = asset_root().join("note_skins").join(name);
    let manifest_path = folder.join("manifest.json");
    let text = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
    let manifest: Manifest = crate::core::jsonc::parse(&text)
        .unwrap_or_else(|error| panic!("invalid {}: {error}", manifest_path.display()));

    let load = |file: &str| asset_server.load::<Image>(format!("note_skins/{name}/{file}"));
    // Hold bodies wrap vertically on the gpu, so a single quad can tile the
    // pattern for any hold length.
    let load_body = |file: &str| {
        asset_server
            .load_builder()
            .with_settings(|settings: &mut ImageLoaderSettings| {
                settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_v: ImageAddressMode::Repeat,
                    ..default()
                });
            })
            .load(format!("note_skins/{name}/{file}"))
    };
    let sheet = load(&manifest.sheet);
    let hold_body_inactive = load_body(&manifest.hold_body.inactive);
    let hold_body_active = load_body(&manifest.hold_body.active);

    let mut layout = TextureAtlasLayout::new_empty(UVec2::new(4096, 4096));
    let mut rect = |pos: [u32; 2], size: [u32; 2]| {
        layout.add_texture(URect::new(
            pos[0],
            pos[1],
            pos[0] + size[0],
            pos[1] + size[1],
        ))
    };

    let taps = &manifest.taps;
    let mut tap_rows = Vec::new();
    for (row, quant) in taps.quants.iter().enumerate() {
        let mut base = None;
        for frame in 0..taps.frames {
            let index = rect(
                [
                    taps.origin[0] + frame as u32 * taps.stride[0],
                    taps.origin[1] + row as u32 * taps.stride[1],
                ],
                taps.frame_size,
            );
            base.get_or_insert(index);
        }
        tap_rows.push((*quant, base.expect("taps have at least one frame")));
    }

    let receptor_frames = [
        rect(manifest.receptor.frames[0], manifest.receptor.size),
        rect(manifest.receptor.frames[1], manifest.receptor.size),
    ];
    let hold_head_inactive = manifest
        .hold_head
        .inactive
        .iter()
        .map(|pos| rect(*pos, manifest.hold_head.size))
        .collect();
    let hold_head_active = manifest
        .hold_head
        .active
        .iter()
        .map(|pos| rect(*pos, manifest.hold_head.size))
        .collect();
    let hold_cap_inactive = rect(manifest.hold_cap.inactive, manifest.hold_cap.size);
    let hold_cap_active = rect(manifest.hold_cap.active, manifest.hold_cap.size);
    let mine = rect(manifest.mine.pos, manifest.mine.size);
    let tap_explosion = rect(manifest.tap_explosion.pos, manifest.tap_explosion.size);
    let mine_explosion = rect(manifest.mine_explosion.pos, manifest.mine_explosion.size);

    let layout = layouts.add(layout);

    ActiveNoteSkin {
        name: name.to_string(),
        sheet,
        layout,
        tap_rows,
        tap_frames: taps.frames,
        tap_beats_per_cycle: taps.beats_per_cycle,
        receptor_frames,
        receptor_beat_split: manifest.receptor.beat_split,
        hold_head_inactive,
        hold_head_active,
        hold_cap_inactive,
        hold_cap_active,
        mine,
        mine_spin_beats: manifest.mine.spin_beats,
        tap_explosion,
        mine_explosion,
        hold_body_inactive,
        hold_body_active,
        hold_body_size: Vec2::new(
            manifest.hold_body.size[0] as f32,
            manifest.hold_body.size[1] as f32,
        ),
        hold_body_stop_above_tail: manifest.hold_body.stop_above_tail,
        hold_cap_size: Vec2::new(
            manifest.hold_cap.size[0] as f32,
            manifest.hold_cap.size[1] as f32,
        ),
        dropped_brightness: manifest.dropped_brightness,
    }
}

/// Reloads the active skin whenever the settings point at a different one
/// (the player options scene edits them).
fn reload_changed_skin(
    settings: Res<Settings>,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut skin: ResMut<ActiveNoteSkin>,
) {
    if !settings.is_changed() || skin.name == settings.stepfile.note_skin {
        return;
    }
    *skin = load_note_skin(&asset_server, &mut layouts, &settings.stepfile.note_skin);
}

/// Lists the note skins on disk by their manifests' display names.
fn scan_note_skins() -> NoteSkinLibrary {
    let root = asset_root().join("note_skins");
    let entries = std::fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()));
    let mut skins = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let manifest_path = entry.path().join("manifest.json");
        let text = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
        let manifest: DisplayNameManifest = crate::core::jsonc::parse(&text)
            .unwrap_or_else(|error| panic!("invalid {}: {error}", manifest_path.display()));
        skins.push(NoteSkinEntry {
            name,
            display_name: manifest.display_name,
        });
    }
    skins.sort_by(|a, b| a.name.cmp(&b.name));
    NoteSkinLibrary { skins }
}

/// The one manifest field the library scan needs.
#[derive(Deserialize)]
struct DisplayNameManifest {
    display_name: String,
}

#[derive(Deserialize)]
struct Manifest {
    sheet: String,
    taps: TapsManifest,
    receptor: ReceptorManifest,
    hold_head: HoldHeadManifest,
    hold_cap: StatePairManifest,
    mine: MineManifest,
    tap_explosion: RegionManifest,
    mine_explosion: RegionManifest,
    hold_body: HoldBodyManifest,
    dropped_brightness: f32,
}

#[derive(Deserialize)]
struct TapsManifest {
    quants: Vec<u32>,
    frames: usize,
    frame_size: [u32; 2],
    origin: [u32; 2],
    stride: [u32; 2],
    beats_per_cycle: f64,
}

#[derive(Deserialize)]
struct ReceptorManifest {
    frames: [[u32; 2]; 2],
    size: [u32; 2],
    beat_split: f64,
}

#[derive(Deserialize)]
struct StatePairManifest {
    inactive: [u32; 2],
    active: [u32; 2],
    size: [u32; 2],
}

#[derive(Deserialize)]
struct HoldHeadManifest {
    inactive: Vec<[u32; 2]>,
    active: Vec<[u32; 2]>,
    size: [u32; 2],
}

#[derive(Deserialize)]
struct MineManifest {
    pos: [u32; 2],
    size: [u32; 2],
    spin_beats: f64,
}

#[derive(Deserialize)]
struct RegionManifest {
    pos: [u32; 2],
    size: [u32; 2],
}

#[derive(Deserialize)]
struct HoldBodyManifest {
    inactive: String,
    active: String,
    size: [u32; 2],
    stop_above_tail: f32,
}
