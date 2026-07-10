use crate::core::assets::asset_root;
use crate::core::jsonc;
use crate::core::platform::platform;
use crate::core::player::{PerPlayer, PlayerId};
use crate::core::settings::PlayerSettings;
use crate::core::units::Beat;
use bevy::asset::UntypedHandle;
use bevy::gltf::GltfAssetLabel;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;
use serde::Deserialize;
use strum::IntoEnumIterator;

/// Skin art is authored on a 64px arrow cell — sprite frames are 64px
/// squares and model meshes span ±32 units — and fields scale it to their
/// arrow size.
pub const NOTE_CELL: f32 = 64.0;

/// Each player's loaded note skin, kept on whatever their settings name.
///
/// Everything a skin draws inside the note field is unlit 3D: either the
/// skin's own mesh (model skins) or a shared unit quad textured through an
/// unlit material (sprite skins) — receptors, notes, holds, mines,
/// flashes, and explosions are all geometry in the lane's own scene, so a
/// lane camera's perspective applies to every one of them consistently.
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
    /// Taps and hold heads.
    pub note: NoteArt,
    pub receptor: ReceptorArt,
    pub hold: HoldArt,
    /// Rolls fall back to the hold art when the skin has none.
    pub roll: HoldArt,
    pub mine: MineArt,
    pub mine_spin_beats: f64,
    pub mine_explosion: SpriteArt,
    pub flash_bright: SpriteArt,
    pub flash_dim: SpriteArt,
    /// Brightness of dropped (NG) hold parts.
    pub dropped_brightness: f32,
    /// The unit quad every sprite-style element is drawn with.
    quad: Handle<Mesh>,
}

impl ActiveNoteSkin {
    pub fn tap_visual(&self, row: usize) -> ElementVisual {
        match &self.note {
            NoteArt::Sheet(sheet) => self.quad_visual(sheet.rows[row].tap.clone()),
            NoteArt::Model(model) => model.visual(row),
        }
    }

    /// Model skins draw hold heads with the tap model in both states.
    pub fn head_visual(&self, row: usize, active: bool) -> ElementVisual {
        match &self.note {
            NoteArt::Sheet(sheet) => self.quad_visual(sheet.rows[row].head(active).clone()),
            NoteArt::Model(model) => model.visual(row),
        }
    }

    pub fn mine_visual(&self) -> ElementVisual {
        match &self.mine {
            MineArt::Sheet(material) => self.quad_visual(material.clone()),
            MineArt::Model(model) => ElementVisual {
                mesh: model.mesh.clone(),
                material: model.material.clone(),
                shell: model.shell.clone(),
                cell: NOTE_CELL,
            },
        }
    }

    pub fn receptor_visual(&self) -> ElementVisual {
        self.quad_visual(self.receptor.material.clone())
    }

    pub fn quad_visual(&self, material: Handle<StandardMaterial>) -> ElementVisual {
        ElementVisual {
            mesh: self.quad.clone(),
            material,
            shell: None,
            cell: 1.0,
        }
    }

    /// Everything that loads asynchronously and must be in before the skin
    /// draws correctly.
    pub fn loading_assets(&self) -> Vec<UntypedHandle> {
        let mut assets = vec![
            self.receptor.image.clone().untyped(),
            self.hold.body_active.clone().untyped(),
            self.hold.body_inactive.clone().untyped(),
            self.mine_explosion.image.clone().untyped(),
        ];
        match &self.note {
            NoteArt::Sheet(sheet) => assets.push(sheet.taps_image.clone().untyped()),
            NoteArt::Model(model) => {
                assets.push(model.mesh.clone().untyped());
                if let Some((mesh, _)) = &model.shell {
                    assets.push(mesh.clone().untyped());
                }
            }
        }
        if let MineArt::Model(model) = &self.mine {
            assets.push(model.mesh.clone().untyped());
        }
        assets
    }

    /// Materials whose texture coordinates drift over time, with their base
    /// offset and velocity in UV per second.
    fn scrolling_materials(&self) -> Vec<(&Handle<StandardMaterial>, Vec2, Vec2)> {
        let mut scrolling = Vec::new();
        if let NoteArt::Model(model) = &self.note {
            for row in &model.rows {
                scrolling.push((
                    &row.material,
                    Vec2::new(row.uv_offset, 0.0),
                    model.uv_scroll,
                ));
            }
        }
        if let MineArt::Model(model) = &self.mine {
            scrolling.push((&model.material, Vec2::ZERO, model.uv_scroll));
        }
        scrolling
    }
}

/// One drawable note-field element: a mesh (an arrow model or the shared
/// unit quad) and its unlit material. `cell` is the mesh's authored size
/// for one arrow cell — display scale is `arrow_size / cell`.
pub struct ElementVisual {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    /// A model's texture-static outline/sheen shell, drawn with the
    /// element and sharing its transform.
    pub shell: Option<(Handle<Mesh>, Handle<StandardMaterial>)>,
    pub cell: f32,
}

/// The note art: a sprite sheet animated by sliding the materials' texture
/// coordinates frame by frame, or a 3D model whose per-quant color comes
/// from a texture-coordinate offset.
#[derive(Clone)]
pub enum NoteArt {
    Sheet(SheetNotes),
    Model(ModelNotes),
}

impl NoteArt {
    /// The skin row for a quant; unknown quants use the last (finest) row.
    pub fn quant_row(&self, quant: u32) -> usize {
        let position = match self {
            NoteArt::Sheet(sheet) => sheet.quants.iter().position(|q| *q == quant),
            NoteArt::Model(model) => model.rows.iter().position(|row| row.quant == quant),
        };
        position.unwrap_or_else(|| match self {
            NoteArt::Sheet(sheet) => sheet.quants.len() - 1,
            NoteArt::Model(model) => model.rows.len() - 1,
        })
    }
}

/// Sprite-sheet notes: the taps texture is a grid with one row per quant
/// and `frames` animation columns; hold-head strips hold one frame per
/// quant. Every tap of a quant shares one material, animated in unison by
/// sliding its texture coordinates.
#[derive(Clone)]
pub struct SheetNotes {
    pub beats_per_cycle: f64,
    frames: usize,
    quants: Vec<u32>,
    rows: Vec<SheetRow>,
    taps_image: Handle<Image>,
}

impl SheetNotes {
    /// The texture-coordinate x of the frame shown at `beat`, for every row
    /// material.
    pub fn frame_x_at(&self, beat: Beat) -> f32 {
        let cycle = beat.0.rem_euclid(self.beats_per_cycle) / self.beats_per_cycle;
        let frame = ((cycle * self.frames as f64) as usize).min(self.frames - 1);
        frame as f32 / self.frames as f32
    }

    pub fn tap_materials(&self) -> impl Iterator<Item = &Handle<StandardMaterial>> {
        self.rows.iter().map(|row| &row.tap)
    }
}

#[derive(Clone)]
struct SheetRow {
    tap: Handle<StandardMaterial>,
    head_inactive: Handle<StandardMaterial>,
    head_active: Handle<StandardMaterial>,
}

impl SheetRow {
    fn head(&self, active: bool) -> &Handle<StandardMaterial> {
        if active {
            &self.head_active
        } else {
            &self.head_inactive
        }
    }
}

/// Model notes: one shared arrow mesh, one unlit material per quant — all
/// pointing into the same texture, apart by `uv_offset` — plus the
/// texture-static shell every quant shares.
#[derive(Clone)]
pub struct ModelNotes {
    mesh: Handle<Mesh>,
    shell: Option<(Handle<Mesh>, Handle<StandardMaterial>)>,
    uv_scroll: Vec2,
    rows: Vec<ModelRow>,
}

impl ModelNotes {
    fn visual(&self, row: usize) -> ElementVisual {
        ElementVisual {
            mesh: self.mesh.clone(),
            material: self.rows[row].material.clone(),
            shell: self.shell.clone(),
            cell: NOTE_CELL,
        }
    }
}

#[derive(Clone)]
struct ModelRow {
    quant: u32,
    uv_offset: f32,
    material: Handle<StandardMaterial>,
}

/// The receptor's animation strip: frames cycle on the beat clock, each
/// shown for its duration in beats, optionally pulsing brightness on the
/// beat.
#[derive(Clone)]
pub struct ReceptorArt {
    pub material: Handle<StandardMaterial>,
    image: Handle<Image>,
    frames: Vec<f64>,
    cycle: f64,
    pulse: Option<f32>,
}

impl ReceptorArt {
    /// The texture-coordinate x of the frame shown at `beat`.
    pub fn frame_x_at(&self, beat: Beat) -> f32 {
        let mut into_cycle = beat.0.rem_euclid(self.cycle);
        let mut frame = 0;
        for (index, duration) in self.frames.iter().enumerate() {
            if into_cycle < *duration {
                frame = index;
                break;
            }
            into_cycle -= duration;
        }
        frame as f32 / self.frames.len() as f32
    }

    /// Full brightness on the beat, decaying to the pulse floor before the
    /// next; constant without a pulse.
    pub fn brightness_at(&self, beat: Beat) -> f32 {
        match self.pulse {
            None => 1.0,
            Some(floor) => floor + (1.0 - floor) * (1.0 - beat.0.rem_euclid(1.0) as f32),
        }
    }
}

/// Hold (or roll) tail art. The parts get their own materials at spawn
/// (their texture windows vary per hold), built from these textures.
#[derive(Clone)]
pub struct HoldArt {
    /// Wraps vertically, so one quad tiles the pattern for any length.
    pub body_active: Handle<Image>,
    pub body_inactive: Handle<Image>,
    pub body_size: Vec2,
    /// The body ends this many texture pixels above the tail center, where
    /// the cap takes over.
    pub stop_above_tail: f32,
    pub cap_active: Handle<Image>,
    pub cap_inactive: Handle<Image>,
    pub cap_size: Vec2,
}

#[derive(Clone)]
pub enum MineArt {
    Sheet(Handle<StandardMaterial>),
    Model(ModelArt),
}

/// A mesh with a single unlit material, plus its optional texture-static
/// shell.
#[derive(Clone)]
pub struct ModelArt {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    shell: Option<(Handle<Mesh>, Handle<StandardMaterial>)>,
    uv_scroll: Vec2,
}

/// A 2D sprite at its native texture size — overlay elements only.
#[derive(Clone)]
pub struct SpriteArt {
    pub image: Handle<Image>,
    pub size: Vec2,
}

/// Lane elements alpha-blend and sort by camera distance; these biases
/// encode the lane scene's stacking rules on top of that, holding under
/// any camera: receptors are the lane floor under everything, hold tails
/// lie under the notes (a head must cover its body's seam even when a
/// tilted camera brings the tall body quad's center closer), and effects
/// (arrow flashes, explosions) burst above everything.
const LANE_FLOOR_DEPTH_BIAS: f32 = -10_000.0;
const LANE_TAIL_DEPTH_BIAS: f32 = -5_000.0;
const LANE_EFFECT_DEPTH_BIAS: f32 = 10_000.0;

/// An unlit alpha-blended material for a note-field quad or model:
/// lighting-free so the art shows exactly as painted, blended for smooth
/// edges and translucency (model meshes ship their triangles ordered
/// back-to-front, so their layers composite correctly without depth).
pub fn unlit_material(texture: Handle<Image>, uv_transform: Affine2) -> StandardMaterial {
    StandardMaterial {
        base_color_texture: Some(texture),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        // Source meshes come with either triangle winding.
        cull_mode: None,
        double_sided: true,
        uv_transform,
        ..default()
    }
}

/// The material of a lane effect: tinted, per-entity (its owner animates
/// the alpha), drawn above everything in the lane.
pub fn effect_material(texture: Handle<Image>, color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        depth_bias: LANE_EFFECT_DEPTH_BIAS,
        ..unlit_material(texture, Affine2::IDENTITY)
    }
}

/// The material of a hold-tail part: per-entity (the hold animator drives
/// its texture window), drawn under the notes.
pub fn tail_material(texture: Handle<Image>) -> StandardMaterial {
    StandardMaterial {
        depth_bias: LANE_TAIL_DEPTH_BIAS,
        ..unlit_material(texture, Affine2::IDENTITY)
    }
}

/// Every note skin found under `assets/note_skins`.
#[derive(Resource)]
pub struct NoteSkinLibrary {
    pub skins: Vec<NoteSkinEntry>,
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
            .add_systems(Update, (reload_changed_skins, scroll_model_textures));
    }
}

/// Panics on a missing or invalid skin: the requested skin must exist.
pub fn load_note_skin(asset_server: &AssetServer, name: &str) -> ActiveNoteSkin {
    let manifest = read_manifest(name);
    let loader = SkinAssetLoader { asset_server, name };
    let quad = asset_server.add(Mesh::from(Rectangle::new(1.0, 1.0)));

    let note = match &manifest.note {
        NoteManifest::Sheet(sheet) => {
            assert!(
                !sheet.quants.is_empty(),
                "note skin {name}: quants must not be empty"
            );
            assert!(
                sheet.frames > 0,
                "note skin {name}: taps need at least one animation frame"
            );
            assert!(
                sheet.beats_per_cycle > 0.0,
                "note skin {name}: beats_per_cycle must be positive"
            );
            let taps_image = loader.image(&sheet.taps);
            let head_inactive = loader.image(&sheet.hold_head_inactive);
            let head_active = loader.image(&sheet.hold_head_active);
            let quants = sheet.quants.len() as f32;
            let rows = (0..sheet.quants.len())
                .map(|row| {
                    let tap_window = Affine2 {
                        matrix2: Mat2::from_diagonal(Vec2::new(
                            1.0 / sheet.frames as f32,
                            1.0 / quants,
                        )),
                        translation: Vec2::new(0.0, row as f32 / quants),
                    };
                    let head_window = Affine2 {
                        matrix2: Mat2::from_diagonal(Vec2::new(1.0 / quants, 1.0)),
                        translation: Vec2::new(row as f32 / quants, 0.0),
                    };
                    SheetRow {
                        tap: loader.material(&taps_image, tap_window),
                        head_inactive: loader.material(&head_inactive, head_window),
                        head_active: loader.material(&head_active, head_window),
                    }
                })
                .collect();
            NoteArt::Sheet(SheetNotes {
                beats_per_cycle: sheet.beats_per_cycle,
                frames: sheet.frames,
                quants: sheet.quants.clone(),
                rows,
                taps_image,
            })
        }
        NoteManifest::Model(model) => NoteArt::Model(loader.model_notes(model)),
    };

    let receptor_image = loader.image(&manifest.receptor.image);
    let frames = &manifest.receptor.frames;
    assert!(
        frames.iter().sum::<f64>() > 0.0,
        "note skin {name}: receptor frames must cover a positive beat span"
    );
    let receptor = ReceptorArt {
        material: loader.floor_material(
            &receptor_image,
            Affine2 {
                matrix2: Mat2::from_diagonal(Vec2::new(1.0 / frames.len() as f32, 1.0)),
                translation: Vec2::ZERO,
            },
        ),
        image: receptor_image,
        cycle: frames.iter().sum(),
        frames: frames.clone(),
        pulse: manifest.receptor.pulse,
    };

    let hold_art = |hold: &HoldManifest| {
        assert!(
            hold.body_size
                .iter()
                .chain(&hold.cap_size)
                .all(|side| *side > 0.0),
            "note skin {name}: hold body and cap sizes must be positive"
        );
        HoldArt {
            body_active: loader.body_image(&hold.body_active),
            body_inactive: loader.body_image(&hold.body_inactive),
            body_size: hold.body_size.into(),
            stop_above_tail: hold.stop_above_tail,
            cap_active: loader.image(&hold.cap_active),
            cap_inactive: loader.image(&hold.cap_inactive),
            cap_size: hold.cap_size.into(),
        }
    };
    let hold = hold_art(&manifest.hold);
    let roll = manifest
        .roll
        .as_ref()
        .map(&hold_art)
        .unwrap_or_else(|| hold.clone());

    assert!(
        manifest.mine.spin_beats > 0.0,
        "note skin {name}: mine spin_beats must be positive"
    );
    let mine = match &manifest.mine.art {
        MineArtManifest::Sheet(sheet) => {
            let image = loader.image(&sheet.image);
            MineArt::Sheet(loader.material(&image, Affine2::IDENTITY))
        }
        MineArtManifest::Model(model) => {
            let texture = loader.wrapping_image(&model.texture);
            MineArt::Model(ModelArt {
                mesh: loader.mesh(&model.mesh, 0),
                material: loader.material(&texture, Affine2::IDENTITY),
                shell: loader.shell(&model.mesh, model.static_shell, &texture),
                uv_scroll: model.uv_scroll.into(),
            })
        }
    };

    ActiveNoteSkin {
        name: name.to_string(),
        note,
        receptor,
        hold,
        roll,
        mine,
        mine_spin_beats: manifest.mine.spin_beats,
        mine_explosion: loader.sprite(&manifest.mine_explosion),
        flash_bright: loader.sprite(&manifest.flash.bright),
        flash_dim: loader.sprite(&manifest.flash.dim),
        dropped_brightness: manifest.dropped_brightness,
        quad,
    }
}

/// Loads a skin's asset files, all addressed relative to its folder.
struct SkinAssetLoader<'a> {
    asset_server: &'a AssetServer,
    name: &'a str,
}

impl SkinAssetLoader<'_> {
    fn path(&self, file: &str) -> String {
        format!("note_skins/{}/{file}", self.name)
    }

    fn image(&self, file: &str) -> Handle<Image> {
        self.asset_server.load(self.path(file))
    }

    /// Wraps on both axes, so scrolling texture coordinates tile.
    fn wrapping_image(&self, file: &str) -> Handle<Image> {
        self.sampled_image(file, ImageAddressMode::Repeat, ImageAddressMode::Repeat)
    }

    /// Wraps vertically, so a single quad can tile a hold body pattern for
    /// any hold length.
    fn body_image(&self, file: &str) -> Handle<Image> {
        self.sampled_image(
            file,
            ImageAddressMode::ClampToEdge,
            ImageAddressMode::Repeat,
        )
    }

    fn sampled_image(
        &self,
        file: &str,
        address_mode_u: ImageAddressMode,
        address_mode_v: ImageAddressMode,
    ) -> Handle<Image> {
        self.asset_server
            .load_builder()
            .with_settings(move |settings: &mut ImageLoaderSettings| {
                // Built on `linear()`, not `default()`: the descriptor's
                // default filters are nearest, which pixelates any scaled
                // art.
                settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u,
                    address_mode_v,
                    ..ImageSamplerDescriptor::linear()
                });
            })
            .load(self.path(file))
    }

    fn mesh(&self, file: &str, primitive: usize) -> Handle<Mesh> {
        self.asset_server
            .load(GltfAssetLabel::Primitive { mesh: 0, primitive }.from_asset(self.path(file)))
    }

    /// The model's texture-static outline/sheen shell — primitive 1 of the
    /// glb per the skin format — with one shared unanimated material.
    fn shell(
        &self,
        file: &str,
        static_shell: bool,
        texture: &Handle<Image>,
    ) -> Option<(Handle<Mesh>, Handle<StandardMaterial>)> {
        static_shell.then(|| {
            (
                self.mesh(file, 1),
                self.material(texture, Affine2::IDENTITY),
            )
        })
    }

    fn material(&self, texture: &Handle<Image>, window: Affine2) -> Handle<StandardMaterial> {
        self.asset_server
            .add(unlit_material(texture.clone(), window))
    }

    /// The receptors' material: sorted as the lane floor, under everything.
    fn floor_material(&self, texture: &Handle<Image>, window: Affine2) -> Handle<StandardMaterial> {
        self.asset_server.add(StandardMaterial {
            depth_bias: LANE_FLOOR_DEPTH_BIAS,
            ..unlit_material(texture.clone(), window)
        })
    }

    fn model_notes(&self, model: &ModelManifest) -> ModelNotes {
        assert!(
            !model.quant_offsets.is_empty(),
            "note skin {}: quant_offsets must not be empty",
            self.name
        );
        let texture = self.wrapping_image(&model.texture);
        ModelNotes {
            mesh: self.mesh(&model.mesh, 0),
            shell: self.shell(&model.mesh, model.static_shell, &texture),
            uv_scroll: model.uv_scroll.into(),
            rows: model
                .quant_offsets
                .iter()
                .map(|(quant, offset)| ModelRow {
                    quant: *quant,
                    uv_offset: *offset,
                    material: self
                        .material(&texture, Affine2::from_translation(Vec2::new(*offset, 0.0))),
                })
                .collect(),
        }
    }

    fn sprite(&self, manifest: &SpriteManifest) -> SpriteArt {
        SpriteArt {
            image: self.image(&manifest.image),
            size: manifest.size.into(),
        }
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

/// Drifts the texture coordinates of every scrolling model material — the
/// classic animated color strips of 3D note skins. Derived from unwrapped
/// f64 time, so it stays idempotent for materials shared between players
/// and never pops at a wrap seam.
fn scroll_model_textures(
    time: Res<Time>,
    skins: Res<ActiveNoteSkins>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let elapsed = time.elapsed_secs_f64();
    for player in PlayerId::iter() {
        for (handle, base, velocity) in skins.get(player).scrolling_materials() {
            if velocity == Vec2::ZERO {
                continue;
            }
            let Some(mut material) = materials.get_mut(handle) else {
                continue;
            };
            let scroll = |base: f32, velocity: f32| {
                (base as f64 + velocity as f64 * elapsed).rem_euclid(1.0) as f32
            };
            material.uv_transform.translation =
                Vec2::new(scroll(base.x, velocity.x), scroll(base.y, velocity.y));
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
        let manifest = read_manifest(&name);
        skins.push(NoteSkinEntry {
            name,
            display_name: manifest.display_name,
        });
    }
    skins.sort_by(|a, b| a.name.cmp(&b.name));
    NoteSkinLibrary { skins }
}

fn read_manifest(name: &str) -> Manifest {
    let path = asset_root()
        .join("note_skins")
        .join(name)
        .join("manifest.json");
    let bytes = platform()
        .read_asset(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    jsonc::parse(&String::from_utf8_lossy(&bytes))
        .unwrap_or_else(|error| panic!("invalid {}: {error}", path.display()))
}

#[derive(Deserialize)]
struct Manifest {
    display_name: String,
    dropped_brightness: f32,
    note: NoteManifest,
    receptor: ReceptorManifest,
    hold: HoldManifest,
    roll: Option<HoldManifest>,
    mine: MineManifest,
    mine_explosion: SpriteManifest,
    flash: FlashManifest,
}

#[derive(Deserialize)]
enum NoteManifest {
    Sheet(SheetManifest),
    Model(ModelManifest),
}

#[derive(Deserialize)]
struct SheetManifest {
    taps: String,
    quants: Vec<u32>,
    frames: usize,
    beats_per_cycle: f64,
    hold_head_active: String,
    hold_head_inactive: String,
}

#[derive(Deserialize)]
struct ModelManifest {
    /// Primitive 0 follows the quant offset and scroll; with
    /// `static_shell`, primitive 1 is the texture-static outline/sheen.
    mesh: String,
    texture: String,
    /// Texture-coordinate U offset selecting each quant's color.
    quant_offsets: Vec<(u32, f32)>,
    /// Texture-coordinate drift in UV per second.
    uv_scroll: [f32; 2],
    static_shell: bool,
}

#[derive(Deserialize)]
struct ReceptorManifest {
    image: String,
    /// Each strip frame's duration in beats.
    frames: Vec<f64>,
    pulse: Option<f32>,
}

#[derive(Deserialize)]
struct HoldManifest {
    body_active: String,
    body_inactive: String,
    body_size: [f32; 2],
    stop_above_tail: f32,
    cap_active: String,
    cap_inactive: String,
    cap_size: [f32; 2],
}

#[derive(Deserialize)]
struct MineManifest {
    art: MineArtManifest,
    spin_beats: f64,
}

#[derive(Deserialize)]
enum MineArtManifest {
    Sheet(ImageManifest),
    Model(MineModelManifest),
}

#[derive(Deserialize)]
struct ImageManifest {
    image: String,
}

#[derive(Deserialize)]
struct MineModelManifest {
    mesh: String,
    texture: String,
    uv_scroll: [f32; 2],
    static_shell: bool,
}

#[derive(Deserialize)]
struct SpriteManifest {
    image: String,
    size: [f32; 2],
}

#[derive(Deserialize)]
struct FlashManifest {
    bright: SpriteManifest,
    dim: SpriteManifest,
}
