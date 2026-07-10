use crate::core::font::label;
use crate::core::input::{Actions, GameAction, StepDirection};
use crate::core::player::PlayerId;
use crate::core::screen::{ACTIVE_COLOR, INACTIVE_COLOR, TITLE_COLOR};
use crate::core::settings::{Settings, VolumeSettings};
use crate::core::sfx::Sfx;
use crate::nodes::menu::NavInput;
use crate::scenes::{
    GameScene, change_scene, play_default_bgm, scene_accepts_input, spawn_default_background,
};
use godot::classes::control::LayoutPreset;
use godot::classes::{CenterContainer, ColorRect, Control, IControl, Label, VBoxContainer};
use godot::prelude::*;
use strum::{EnumCount, EnumIter, IntoEnumIterator, IntoStaticStr};

/// The audio settings scene: one volume slider per bus, edited in place
/// (they live in the machine settings, so changes persist immediately).
/// ¤Up¤/¤Down¤ pick a slider, ¤Left¤/¤Right¤ adjust it — audible right
/// away on the scene's own music and navigation sounds.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct AudioSettingsScene {
    active: usize,
    rows: Vec<SliderRow>,
    base: Base<Control>,
}

#[derive(Debug, Clone, Copy, PartialEq, EnumCount, EnumIter, IntoStaticStr)]
enum VolumeKind {
    Master,
    #[strum(serialize = "SFX")]
    Sfx,
    Music,
}

impl VolumeKind {
    fn get(self, volume: &VolumeSettings) -> f32 {
        match self {
            VolumeKind::Master => volume.master,
            VolumeKind::Sfx => volume.sfx,
            VolumeKind::Music => volume.music,
        }
    }

    fn set(self, volume: &mut VolumeSettings, value: f32) {
        match self {
            VolumeKind::Master => volume.master = value,
            VolumeKind::Sfx => volume.sfx = value,
            VolumeKind::Music => volume.music = value,
        }
    }
}

fn kind(index: usize) -> VolumeKind {
    VolumeKind::iter().nth(index).expect("row index is wrapped")
}

const VOLUME_STEP: f32 = 0.05;
const SLIDER_WIDTH: f32 = 360.0;
const SLIDER_HEIGHT: f32 = 14.0;
const SLIDER_PADDING: f32 = 2.0;

struct SliderRow {
    label: Gd<Label>,
    fill: Gd<ColorRect>,
    value: Gd<Label>,
}

#[godot_api]
impl AudioSettingsScene {
    pub fn instantiate() -> Gd<AudioSettingsScene> {
        let mut scene = AudioSettingsScene::new_alloc();
        scene.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        play_default_bgm();
        spawn_default_background(&mut scene.clone().upcast::<Control>());

        let mut center = CenterContainer::new_alloc();
        center.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        let mut column = VBoxContainer::new_alloc();
        column.add_theme_constant_override("separation", 20);
        let mut title = label("Audio Settings", 52.0, TITLE_COLOR);
        title.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        column.add_child(&title);
        let mut spacer = Control::new_alloc();
        spacer.set_custom_minimum_size(Vector2::new(0.0, 12.0));
        column.add_child(&spacer);

        let settings = Settings::singleton();
        let volume = settings.bind().machine().volume.clone();
        let mut rows = Vec::new();
        for kind in VolumeKind::iter() {
            let mut row = godot::classes::HBoxContainer::new_alloc();
            row.add_theme_constant_override("separation", 24);

            let mut name_cell = CenterContainer::new_alloc();
            name_cell.set_custom_minimum_size(Vector2::new(160.0, 40.0));
            let name = label(kind.into(), 34.0, INACTIVE_COLOR);
            name_cell.add_child(&name);
            row.add_child(&name_cell);

            let mut track = ColorRect::new_alloc();
            track.set_color(Color::from_rgba(1.0, 1.0, 1.0, 0.12));
            track.set_custom_minimum_size(Vector2::new(SLIDER_WIDTH, SLIDER_HEIGHT));
            track.set_v_size_flags(godot::classes::control::SizeFlags::SHRINK_CENTER);
            let mut fill = ColorRect::new_alloc();
            fill.set_color(INACTIVE_COLOR);
            fill.set_position(Vector2::splat(SLIDER_PADDING));
            fill.set_size(fill_size(kind.get(&volume)));
            track.add_child(&fill);
            row.add_child(&track);

            let mut value_cell = CenterContainer::new_alloc();
            value_cell.set_custom_minimum_size(Vector2::new(80.0, 40.0));
            let value = label(
                &format!("{:.0}%", kind.get(&volume) * 100.0),
                28.0,
                INACTIVE_COLOR,
            );
            value_cell.add_child(&value);
            row.add_child(&value_cell);

            column.add_child(&row);
            rows.push(SliderRow {
                label: name,
                fill,
                value,
            });
        }
        center.add_child(&column);
        scene.add_child(&center);
        scene.bind_mut().rows = rows;
        scene
    }

    /// ¤Left¤/¤Right¤ (with hold-repeat) step the active slider; the change
    /// is audible immediately, and the navigation blip doubles as an
    /// SFX-volume sample.
    fn handle_pulses(&mut self) {
        for pulse in NavInput::pulses() {
            let Some((PlayerId::P1, direction)) = pulse.as_step() else {
                continue;
            };
            match direction {
                StepDirection::Up => {
                    self.active = (self.active + VolumeKind::COUNT - 1) % VolumeKind::COUNT;
                    Sfx::Navigate.play();
                }
                StepDirection::Down => {
                    self.active = (self.active + 1) % VolumeKind::COUNT;
                    Sfx::Navigate.play();
                }
                StepDirection::Left | StepDirection::Right => {
                    let delta = if direction == StepDirection::Left {
                        -VOLUME_STEP
                    } else {
                        VOLUME_STEP
                    };
                    let volume = kind(self.active);
                    let mut settings = Settings::singleton();
                    let current = volume.get(&settings.bind().machine().volume);
                    // Snapped to whole percent so drifting float sums never
                    // show as ragged percentages (or serialize as them).
                    let stepped = (((current + delta) * 100.0).round() / 100.0).clamp(0.0, 1.0);
                    if stepped != current {
                        settings
                            .bind_mut()
                            .edit_machine(|machine| volume.set(&mut machine.volume, stepped));
                        Sfx::Navigate.play();
                    }
                }
            }
        }
    }

    fn refresh(&mut self) {
        let settings = Settings::singleton();
        let volume = settings.bind().machine().volume.clone();
        for (index, row) in self.rows.iter_mut().enumerate() {
            let color = if index == self.active {
                ACTIVE_COLOR
            } else {
                INACTIVE_COLOR
            };
            row.label.add_theme_color_override("font_color", color);
            row.value.add_theme_color_override("font_color", color);
            row.fill.set_color(color);
            row.fill.set_size(fill_size(kind(index).get(&volume)));
            row.value
                .set_text(&format!("{:.0}%", kind(index).get(&volume) * 100.0));
        }
    }
}

fn fill_size(fraction: f32) -> Vector2 {
    Vector2::new(
        (SLIDER_WIDTH - 2.0 * SLIDER_PADDING) * fraction,
        SLIDER_HEIGHT - 2.0 * SLIDER_PADDING,
    )
}

#[godot_api]
impl IControl for AudioSettingsScene {
    fn init(base: Base<Control>) -> AudioSettingsScene {
        AudioSettingsScene {
            active: 0,
            rows: Vec::new(),
            base,
        }
    }

    fn process(&mut self, _delta: f64) {
        if !scene_accepts_input() {
            return;
        }
        self.handle_pulses();
        if Actions::just_pressed(GameAction::cancel(PlayerId::P1)) {
            Sfx::Cancel.play();
            change_scene(GameScene::SettingsMenu);
        }
        self.refresh();
    }
}
