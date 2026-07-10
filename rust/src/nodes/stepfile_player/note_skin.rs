use crate::core::assets::asset_root;
use crate::core::jsonc;
use crate::core::platform::platform;
use crate::core::units::Beat;
use godot::classes::base_material_3d::{
    CullMode, Flags, ShadingMode, TextureFilter, TextureParam, Transparency,
};
use godot::classes::{
    GltfDocument, GltfState, Image, ImageTexture, Mesh, MeshInstance3D, Node, QuadMesh,
    StandardMaterial3D, Texture2D,
};
use godot::prelude::*;
use serde::Deserialize;

/// Skin art is authored on a 64px arrow cell — sprite frames are 64px
/// squares and model meshes span ±32 units — and fields scale it to their
/// arrow size.
pub const NOTE_CELL: f32 = 64.0;

/// Transparent lane elements sort by camera distance; these priorities
/// encode the lane scene's stacking rules on top of that, holding under
/// any camera: receptors are the lane floor under everything, hold tails
/// lie under the notes (a head must cover its body's seam even when a
/// tilted camera brings the tall body quad's center closer), a model's
/// texture-static shell sits right above its own fill, and effects (arrow
/// flashes, explosions) burst above everything.
const LANE_FLOOR_PRIORITY: i32 = -100;
const LANE_TAIL_PRIORITY: i32 = -50;
const LANE_SHELL_PRIORITY: i32 = 1;
const LANE_EFFECT_PRIORITY: i32 = 100;

/// One loaded note skin. Everything a skin draws inside the note field is
/// unshaded 3D: either the skin's own mesh (model skins) or a shared unit
/// quad textured through an unshaded material (sprite skins) — receptors,
/// notes, holds, mines, flashes, and explosions are all geometry in the
/// lane's own scene, so a lane camera's perspective applies to every one
/// of them consistently.
#[derive(Clone)]
pub struct NoteSkin {
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
    quad: Gd<QuadMesh>,
}

impl NoteSkin {
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

    pub fn quad_visual(&self, material: Gd<StandardMaterial3D>) -> ElementVisual {
        ElementVisual {
            mesh: self.quad.clone().upcast::<Mesh>(),
            material,
            shell: None,
            cell: 1.0,
        }
    }

    /// Materials whose texture coordinates drift over time, with their base
    /// offset and velocity in UV per second.
    pub fn scrolling_materials(&self) -> Vec<(Gd<StandardMaterial3D>, Vector2, Vector2)> {
        let mut scrolling = Vec::new();
        if let NoteArt::Model(model) = &self.note {
            for row in &model.rows {
                scrolling.push((
                    row.material.clone(),
                    Vector2::new(row.uv_offset, 0.0),
                    model.uv_scroll,
                ));
            }
        }
        if let MineArt::Model(model) = &self.mine {
            scrolling.push((model.material.clone(), Vector2::ZERO, model.uv_scroll));
        }
        scrolling
    }
}

/// One drawable note-field element: a mesh (an arrow model or the shared
/// unit quad) and its unshaded material. `cell` is the mesh's authored size
/// for one arrow cell — display scale is `arrow_size / cell`.
pub struct ElementVisual {
    pub mesh: Gd<Mesh>,
    pub material: Gd<StandardMaterial3D>,
    /// A model's texture-static outline/sheen shell material, drawn as the
    /// mesh's second surface.
    pub shell: Option<Gd<StandardMaterial3D>>,
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
}

impl SheetNotes {
    /// The texture-coordinate x of the frame shown at `beat`, for every row
    /// material.
    pub fn frame_x_at(&self, beat: Beat) -> f32 {
        let cycle = beat.0.rem_euclid(self.beats_per_cycle) / self.beats_per_cycle;
        let frame = ((cycle * self.frames as f64) as usize).min(self.frames - 1);
        frame as f32 / self.frames as f32
    }

    pub fn tap_materials(&self) -> impl Iterator<Item = &Gd<StandardMaterial3D>> {
        self.rows.iter().map(|row| &row.tap)
    }
}

#[derive(Clone)]
struct SheetRow {
    tap: Gd<StandardMaterial3D>,
    head_inactive: Gd<StandardMaterial3D>,
    head_active: Gd<StandardMaterial3D>,
}

impl SheetRow {
    fn head(&self, active: bool) -> &Gd<StandardMaterial3D> {
        if active {
            &self.head_active
        } else {
            &self.head_inactive
        }
    }
}

/// Model notes: one shared arrow mesh, one unshaded material per quant — all
/// pointing into the same texture, apart by `uv_offset` — plus the
/// texture-static shell every quant shares (the mesh's second surface).
#[derive(Clone)]
pub struct ModelNotes {
    mesh: Gd<Mesh>,
    shell: Option<Gd<StandardMaterial3D>>,
    uv_scroll: Vector2,
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
    material: Gd<StandardMaterial3D>,
}

/// The receptor's animation strip: frames cycle on the beat clock, each
/// shown for its duration in beats, optionally pulsing brightness on the
/// beat.
#[derive(Clone)]
pub struct ReceptorArt {
    pub material: Gd<StandardMaterial3D>,
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
    pub body_active: Gd<Texture2D>,
    pub body_inactive: Gd<Texture2D>,
    pub body_size: Vector2,
    /// The body ends this many texture pixels above the tail center, where
    /// the cap takes over.
    pub stop_above_tail: f32,
    pub cap_active: Gd<Texture2D>,
    pub cap_inactive: Gd<Texture2D>,
    pub cap_size: Vector2,
}

#[derive(Clone)]
pub enum MineArt {
    Sheet(Gd<StandardMaterial3D>),
    Model(ModelArt),
}

/// A mesh with a single unshaded material, plus its optional texture-static
/// shell surface material.
#[derive(Clone)]
pub struct ModelArt {
    pub mesh: Gd<Mesh>,
    pub material: Gd<StandardMaterial3D>,
    shell: Option<Gd<StandardMaterial3D>>,
    uv_scroll: Vector2,
}

/// A 2D sprite texture at its native size — flashes and explosions.
#[derive(Clone)]
pub struct SpriteArt {
    pub texture: Gd<Texture2D>,
    pub size: Vector2,
}

/// An unshaded alpha-blended material for a note-field quad or model:
/// lighting-free so the art shows exactly as painted, blended for smooth
/// edges and translucency (model meshes ship their triangles ordered
/// back-to-front, so their layers composite correctly without depth).
pub fn unlit_material(
    texture: &Gd<Texture2D>,
    uv_scale: Vector2,
    uv_offset: Vector2,
) -> Gd<StandardMaterial3D> {
    let mut material = StandardMaterial3D::new_gd();
    material.set_shading_mode(ShadingMode::UNSHADED);
    material.set_transparency(Transparency::ALPHA);
    // Source meshes come with either triangle winding.
    material.set_cull_mode(CullMode::DISABLED);
    material.set_texture_filter(TextureFilter::LINEAR);
    material.set_flag(Flags::ALBEDO_TEXTURE_FORCE_SRGB, false);
    material.set_flag(Flags::USE_TEXTURE_REPEAT, true);
    material.set_texture(TextureParam::ALBEDO, texture);
    material.set_uv1_scale(Vector3::new(uv_scale.x, uv_scale.y, 1.0));
    material.set_uv1_offset(Vector3::new(uv_offset.x, uv_offset.y, 0.0));
    material
}

/// The material of a lane effect: tinted, per-entity (its owner animates
/// the alpha), drawn above everything in the lane.
pub fn effect_material(texture: &Gd<Texture2D>, color: Color) -> Gd<StandardMaterial3D> {
    let mut material = unlit_material(texture, Vector2::ONE, Vector2::ZERO);
    material.set_albedo(color);
    material.set_render_priority(LANE_EFFECT_PRIORITY);
    material
}

/// The material of a hold-tail part: per-entity (the hold animator drives
/// its texture window), drawn under the notes.
pub fn tail_material(texture: &Gd<Texture2D>) -> Gd<StandardMaterial3D> {
    let mut material = unlit_material(texture, Vector2::ONE, Vector2::ZERO);
    material.set_render_priority(LANE_TAIL_PRIORITY);
    material
}

/// Every note skin found under `assets/note_skins`, for the options list.
pub struct NoteSkinLibrary {
    pub skins: Vec<NoteSkinEntry>,
}

pub struct NoteSkinEntry {
    /// Folder name under `assets/note_skins`.
    pub name: String,
    pub display_name: String,
}

/// The scanned skin list, set once by the boot sequence via
/// [`NoteSkinLibrary::install`].
pub fn note_skins() -> &'static NoteSkinLibrary {
    NOTE_SKINS.get().expect("NoteSkinLibrary installed at boot")
}

static NOTE_SKINS: std::sync::OnceLock<NoteSkinLibrary> = std::sync::OnceLock::new();

impl NoteSkinLibrary {
    pub fn install() {
        NOTE_SKINS
            .set(scan_note_skins())
            .unwrap_or_else(|_| panic!("NoteSkinLibrary is already installed"));
    }
}

/// Scans the note skin folders; panics when none exist — the game cannot
/// draw notes without them.
pub fn scan_note_skins() -> NoteSkinLibrary {
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

/// Panics on a missing or invalid skin: the requested skin must exist.
pub fn load_note_skin(name: &str) -> NoteSkin {
    let manifest = read_manifest(name);
    let loader = SkinAssetLoader { name };
    let mut quad = QuadMesh::new_gd();
    quad.set_size(Vector2::ONE);

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
            let taps_image = loader.texture(&sheet.taps);
            let head_inactive = loader.texture(&sheet.hold_head_inactive);
            let head_active = loader.texture(&sheet.hold_head_active);
            let quants = sheet.quants.len() as f32;
            let rows = (0..sheet.quants.len())
                .map(|row| {
                    let tap_scale = Vector2::new(1.0 / sheet.frames as f32, 1.0 / quants);
                    let tap_offset = Vector2::new(0.0, row as f32 / quants);
                    let head_scale = Vector2::new(1.0 / quants, 1.0);
                    let head_offset = Vector2::new(row as f32 / quants, 0.0);
                    SheetRow {
                        tap: unlit_material(&taps_image, tap_scale, tap_offset),
                        head_inactive: unlit_material(&head_inactive, head_scale, head_offset),
                        head_active: unlit_material(&head_active, head_scale, head_offset),
                    }
                })
                .collect();
            NoteArt::Sheet(SheetNotes {
                beats_per_cycle: sheet.beats_per_cycle,
                frames: sheet.frames,
                quants: sheet.quants.clone(),
                rows,
            })
        }
        NoteManifest::Model(model) => NoteArt::Model(loader.model_notes(model)),
    };

    let receptor_image = loader.texture(&manifest.receptor.image);
    let frames = &manifest.receptor.frames;
    assert!(
        frames.iter().sum::<f64>() > 0.0,
        "note skin {name}: receptor frames must cover a positive beat span"
    );
    let mut receptor_material = unlit_material(
        &receptor_image,
        Vector2::new(1.0 / frames.len() as f32, 1.0),
        Vector2::ZERO,
    );
    receptor_material.set_render_priority(LANE_FLOOR_PRIORITY);
    let receptor = ReceptorArt {
        material: receptor_material,
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
            body_active: loader.texture(&hold.body_active),
            body_inactive: loader.texture(&hold.body_inactive),
            body_size: Vector2::new(hold.body_size[0], hold.body_size[1]),
            stop_above_tail: hold.stop_above_tail,
            cap_active: loader.texture(&hold.cap_active),
            cap_inactive: loader.texture(&hold.cap_inactive),
            cap_size: Vector2::new(hold.cap_size[0], hold.cap_size[1]),
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
            let image = loader.texture(&sheet.image);
            MineArt::Sheet(unlit_material(&image, Vector2::ONE, Vector2::ZERO))
        }
        MineArtManifest::Model(model) => {
            let texture = loader.texture(&model.texture);
            MineArt::Model(ModelArt {
                mesh: loader.mesh(&model.mesh),
                material: unlit_material(&texture, Vector2::ONE, Vector2::ZERO),
                shell: loader.shell(model.static_shell, &texture),
                uv_scroll: Vector2::new(model.uv_scroll[0], model.uv_scroll[1]),
            })
        }
    };

    NoteSkin {
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
    name: &'a str,
}

impl SkinAssetLoader<'_> {
    fn path(&self, file: &str) -> std::path::PathBuf {
        asset_root().join(format!("note_skins/{}/{file}", self.name))
    }

    fn texture(&self, file: &str) -> Gd<Texture2D> {
        let path = self.path(file);
        let bytes = platform()
            .read_asset(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let mut image = Image::new_gd();
        let ok = image.load_png_from_buffer(&PackedByteArray::from(bytes.as_slice()));
        assert!(
            ok == godot::global::Error::OK,
            "note skin {}: {} is not a valid png",
            self.name,
            path.display()
        );
        ImageTexture::create_from_image(&image)
            .unwrap_or_else(|| panic!("cannot create a texture for {}", path.display()))
            .upcast()
    }

    fn mesh(&self, file: &str) -> Gd<Mesh> {
        let path = self.path(file);
        let bytes = platform()
            .read_asset(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let mut doc = GltfDocument::new_gd();
        let state = GltfState::new_gd();
        let ok = doc.append_from_buffer(&PackedByteArray::from(bytes.as_slice()), "", &state);
        assert!(
            ok == godot::global::Error::OK,
            "note skin {}: {} is not a valid glb",
            self.name,
            path.display()
        );
        let scene = doc
            .generate_scene(&state)
            .unwrap_or_else(|| panic!("{} holds no scene", path.display()));
        let mesh = find_mesh(&scene).unwrap_or_else(|| panic!("{} holds no mesh", path.display()));
        scene.free();
        mesh
    }

    /// The model's texture-static outline/sheen shell — the glb's second
    /// surface per the skin format — as one shared unanimated material.
    fn shell(&self, static_shell: bool, texture: &Gd<Texture2D>) -> Option<Gd<StandardMaterial3D>> {
        static_shell.then(|| {
            let mut material = unlit_material(texture, Vector2::ONE, Vector2::ZERO);
            material.set_render_priority(LANE_SHELL_PRIORITY);
            material
        })
    }

    fn model_notes(&self, model: &ModelManifest) -> ModelNotes {
        assert!(
            !model.quant_offsets.is_empty(),
            "note skin {}: quant_offsets must not be empty",
            self.name
        );
        let texture = self.texture(&model.texture);
        ModelNotes {
            mesh: self.mesh(&model.mesh),
            shell: self.shell(model.static_shell, &texture),
            uv_scroll: Vector2::new(model.uv_scroll[0], model.uv_scroll[1]),
            rows: model
                .quant_offsets
                .iter()
                .map(|(quant, offset)| ModelRow {
                    quant: *quant,
                    uv_offset: *offset,
                    material: unlit_material(&texture, Vector2::ONE, Vector2::new(*offset, 0.0)),
                })
                .collect(),
        }
    }

    fn sprite(&self, manifest: &SpriteManifest) -> SpriteArt {
        SpriteArt {
            texture: self.texture(&manifest.image),
            size: Vector2::new(manifest.size[0], manifest.size[1]),
        }
    }
}

/// The first mesh in the generated scene tree, depth first.
fn find_mesh(scene: &Gd<Node>) -> Option<Gd<Mesh>> {
    if let Ok(instance) = scene.clone().try_cast::<MeshInstance3D>()
        && let Some(mesh) = instance.get_mesh()
    {
        return Some(mesh);
    }
    for child in scene.get_children().iter_shared() {
        if let Some(mesh) = find_mesh(&child) {
            return Some(mesh);
        }
    }
    None
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
    /// Surface 0 follows the quant offset and scroll; with `static_shell`,
    /// surface 1 is the texture-static outline/sheen.
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
