use crate::core::assets::asset_root;
use crate::core::platform::platform;
use crate::core::player::{PerPlayer, PlayerId};
use crate::core::settings::PlayerSettings;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Rect;
use bevy::prelude::*;
use serde::Deserialize;
use strum::IntoEnumIterator;

/// Each player's loaded note skin, kept on whatever their settings name.
#[derive(Resource)]
pub struct ActiveNoteSkins(PerPlayer<ActiveNoteSkin>);

impl ActiveNoteSkins {
    /// Both players on one skin — for tools that render a single field.
    pub fn shared(skin: ActiveNoteSkin) -> ActiveNoteSkins {
        ActiveNoteSkins(PerPlayer {
            p1: skin.clone(),
            p2: skin,
        })
    }

    pub fn get(&self, player: PlayerId) -> &ActiveNoteSkin {
        &self.0[player]
    }
}

#[derive(Clone)]
pub struct ActiveNoteSkin {
    /// Folder name under `assets/note_skins`.
    pub name: String,
    pub sheet: Handle<Image>,
    /// Sheet regions; every sprite index below points into this.
    frames: Vec<Rect>,
    tap_rows: Vec<TapRow>,
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
    pub mine_explosion: usize,
    pub arrow_flash_bright: ArrowFlashSprite,
    pub arrow_flash_dim: ArrowFlashSprite,
    pub hold_body_inactive: Handle<Image>,
    pub hold_body_active: Handle<Image>,
    pub hold_body_size: Vec2,
    /// The body ends this many texture pixels above the tail center, where
    /// the cap takes over.
    pub hold_body_stop_above_tail: f32,
    pub hold_cap_size: Vec2,
    /// Brightness of dropped (NG) hold parts.
    pub dropped_brightness: f32,
}

impl ActiveNoteSkin {
    pub fn frame(&self, index: usize) -> Rect {
        self.frames[index]
    }

    /// The skin row for a quant; unknown quants use the last (finest) row.
    pub fn quant_row(&self, quant: u32) -> usize {
        self.tap_rows
            .iter()
            .position(|row| row.quant == quant)
            .unwrap_or(self.tap_rows.len() - 1)
    }

    pub fn tap_base(&self, row: usize) -> usize {
        self.tap_rows[row.min(self.tap_rows.len() - 1)].first_frame
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

/// One arrow flash sprite: its sheet frame and native square size in
/// texture pixels (display size scales from the 64px arrow cell).
#[derive(Clone, Copy)]
pub struct ArrowFlashSprite {
    pub frame: usize,
    pub size: f32,
}

/// Every note skin found under `assets/note_skins`.
#[derive(Resource)]
pub struct NoteSkinLibrary {
    pub skins: Vec<NoteSkinEntry>,
}

#[derive(Clone)]
struct TapRow {
    quant: u32,
    first_frame: usize,
}

pub struct NoteSkinEntry {
    /// Folder name under `assets/note_skins`.
    pub name: String,
    pub display_name: String,
}

/// Keeps every player's active skin on whatever their settings name:
/// loaded at startup and reloaded whenever they change. Requires
/// [`PlayerSettings`] to already be inserted.
pub struct NoteSkinPlugin;

impl Plugin for NoteSkinPlugin {
    fn build(&self, app: &mut App) {
        let settings = app.world().resource::<PlayerSettings>().clone();
        let asset_server = app.world().resource::<AssetServer>();
        let skins = ActiveNoteSkins(PerPlayer {
            p1: load_note_skin(asset_server, &settings[PlayerId::P1].note_skin),
            p2: load_note_skin(asset_server, &settings[PlayerId::P2].note_skin),
        });
        app.insert_resource(skins)
            .insert_resource(scan_note_skins())
            .add_systems(Update, reload_changed_skins);
    }
}

/// Panics on a missing or invalid skin: the requested skin must exist.
pub fn load_note_skin(asset_server: &AssetServer, name: &str) -> ActiveNoteSkin {
    let folder = asset_root().join("note_skins").join(name);
    let manifest_path = folder.join("manifest.json");
    let bytes = platform()
        .read_asset(&manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
    let manifest: Manifest = crate::core::jsonc::parse(&String::from_utf8_lossy(&bytes))
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

    let mut frames = Vec::new();
    let mut rect = |pos: [u32; 2], size: [u32; 2]| {
        frames.push(Rect::new(
            pos[0] as f32,
            pos[1] as f32,
            (pos[0] + size[0]) as f32,
            (pos[1] + size[1]) as f32,
        ));
        frames.len() - 1
    };

    let taps = &manifest.taps;
    let mut tap_rows = Vec::new();
    for (row, quant) in taps.quants.iter().enumerate() {
        let mut first_frame = None;
        for frame in 0..taps.frames {
            let index = rect(
                [
                    taps.origin[0] + frame as u32 * taps.stride[0],
                    taps.origin[1] + row as u32 * taps.stride[1],
                ],
                taps.frame_size,
            );
            first_frame.get_or_insert(index);
        }
        tap_rows.push(TapRow {
            quant: *quant,
            first_frame: first_frame.expect("taps have at least one frame"),
        });
    }

    let receptor_frames = [
        rect(manifest.receptor.frames[0], manifest.receptor.size),
        rect(manifest.receptor.frames[1], manifest.receptor.size),
    ];
    assert!(
        !manifest.hold_head.inactive.is_empty() && !manifest.hold_head.active.is_empty(),
        "{}: hold_head needs at least one inactive and one active sprite",
        manifest_path.display()
    );
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
    let mine_explosion = rect(manifest.mine_explosion.pos, manifest.mine_explosion.size);
    assert!(
        !tap_rows.is_empty(),
        "{}: taps.quants must not be empty",
        manifest_path.display()
    );

    let mut flash = |region: &RegionManifest| ArrowFlashSprite {
        frame: rect(region.pos, region.size),
        size: region.size[0] as f32,
    };
    let arrow_flash_bright = flash(&manifest.arrow_flash.bright);
    let arrow_flash_dim = flash(&manifest.arrow_flash.dim);

    ActiveNoteSkin {
        name: name.to_string(),
        sheet,
        frames,
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
        mine_explosion,
        arrow_flash_bright,
        arrow_flash_dim,
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

fn reload_changed_skins(
    settings: Res<PlayerSettings>,
    asset_server: Res<AssetServer>,
    mut skins: ResMut<ActiveNoteSkins>,
) {
    if !settings.is_changed() {
        return;
    }
    for player in PlayerId::iter() {
        let wanted = &settings[player].note_skin;
        if skins.0[player].name != *wanted {
            skins.0[player] = load_note_skin(&asset_server, wanted);
        }
    }
}

fn scan_note_skins() -> NoteSkinLibrary {
    let root = asset_root().join("note_skins");
    let entries = platform().list_asset_dir(&root);
    if entries.is_empty() {
        panic!("failed to read {}: no note skins found", root.display());
    }
    let mut skins = Vec::new();
    for entry in entries {
        if !entry.is_dir {
            continue;
        }
        let Some(name) = entry.path.file_name() else {
            continue;
        };
        let name = name.to_string_lossy().to_string();
        let manifest_path = entry.path.join("manifest.json");
        let bytes = platform()
            .read_asset(&manifest_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
        let manifest: DisplayNameManifest =
            crate::core::jsonc::parse(&String::from_utf8_lossy(&bytes))
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
    mine_explosion: RegionManifest,
    arrow_flash: ArrowFlashManifest,
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
struct ArrowFlashManifest {
    bright: RegionManifest,
    dim: RegionManifest,
}

#[derive(Deserialize)]
struct HoldBodyManifest {
    inactive: String,
    active: String,
    size: [u32; 2],
    stop_above_tail: f32,
}
