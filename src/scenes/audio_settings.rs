use crate::core::font::game_font;
use crate::core::input::{Actions, GameAction, StepDirection};
use crate::core::player::PlayerId;
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::{MachineSettings, VolumeSettings};
use crate::core::sfx::{PlaySfx, Sfx};
use crate::prefabs::menu::{ACTIVE_COLOR, INACTIVE_COLOR, Menu, NavPulse, TITLE_COLOR};
use crate::scenes::{
    GameScene, SceneFade, play_default_bgm, scene_accepts_input, spawn_default_background,
};
use bevy::prelude::*;
use strum::{EnumCount, EnumIter, IntoEnumIterator, IntoStaticStr};

/// The audio settings scene: one volume slider per bus, edited in place
/// (they live in the machine settings, so changes persist immediately).
/// ¤Up¤/¤Down¤ pick a slider, ¤Left¤/¤Right¤ adjust it — audible right
/// away on the scene's own music and navigation sounds.
pub(super) struct AudioSettingsPlugin;

impl Plugin for AudioSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(GameScene::AudioSettings),
            (enter, play_default_bgm, spawn_default_background),
        )
        .add_systems(
            Update,
            (adjust_active_volume, handle_cancel, refresh_sliders)
                .chain()
                .run_if(in_state(GameScene::AudioSettings).and_then(scene_accepts_input)),
        );
    }
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

/// The slider parts of one volume row, indexed like the rows.
#[derive(Component, Clone, FromTemplate)]
struct SliderLabel(usize);

#[derive(Component, Clone, FromTemplate)]
struct SliderFill(usize);

#[derive(Component, Clone, FromTemplate)]
struct SliderValue(usize);

fn enter(mut commands: Commands, settings: Res<MachineSettings>) {
    let rows: Vec<_> = VolumeKind::iter()
        .enumerate()
        .map(|(index, volume)| {
            let label: &str = volume.into();
            let fill = Val::Percent(volume.get(&settings.volume) * 100.0);
            let value = format!("{:.0}%", volume.get(&settings.volume) * 100.0);
            bsn! {
                Node { column_gap: px(24), align_items: AlignItems::Center }
                Children [
                    (
                        Node { width: px(160) }
                        Children [(
                            SliderLabel(index)
                            game_font(34.0)
                            Text({label.to_string()})
                            TextColor({INACTIVE_COLOR})
                        )]
                    ),
                    (
                        Node {
                            width: px(SLIDER_WIDTH),
                            height: px(14),
                            padding: {UiRect::all(Val::Px(2.0))},
                        }
                        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.12))
                        Children [(
                            SliderFill(index)
                            Node { width: {fill}, height: percent(100) }
                            BackgroundColor({INACTIVE_COLOR})
                        )]
                    ),
                    (
                        Node { width: px(80) }
                        Children [(
                            SliderValue(index)
                            game_font(28.0)
                            Text({value})
                            TextColor({INACTIVE_COLOR})
                        )]
                    ),
                ]
            }
        })
        .collect();
    commands.spawn_scoped(
        GameScene::AudioSettings,
        bsn! {
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(20),
            }
            Children [
                (
                    game_font(52.0)
                    Text("Audio Settings")
                    TextColor({TITLE_COLOR})
                    Node { margin: {UiRect::bottom(Val::Px(32.0))} }
                ),
                (
                    Menu { active: 0, len: {VolumeKind::COUNT} }
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::FlexStart,
                        row_gap: px(18),
                    }
                    Children [ {rows} ]
                ),
            ]
        },
    );
}

/// ¤Left¤/¤Right¤ (with hold-repeat) step the active slider; the change is
/// audible immediately, and the navigation blip doubles as an SFX-volume
/// sample.
fn adjust_active_volume(
    mut pulses: MessageReader<NavPulse>,
    menus: Query<&Menu>,
    mut settings: ResMut<MachineSettings>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let Ok(menu) = menus.single() else { return };
    for pulse in pulses.read() {
        let Some((PlayerId::P1, direction)) = pulse.action.as_step() else {
            continue;
        };
        let delta = match direction {
            StepDirection::Left => -VOLUME_STEP,
            StepDirection::Right => VOLUME_STEP,
            _ => continue,
        };
        let volume = kind(menu.active);
        let current = volume.get(&settings.volume);
        // Snapped to whole percent so drifting float sums never show as
        // ragged percentages (or serialize as them).
        let stepped = (((current + delta) * 100.0).round() / 100.0).clamp(0.0, 1.0);
        if stepped != current {
            volume.set(&mut settings.volume, stepped);
            sfx.write(PlaySfx(Sfx::Navigate));
        }
    }
}

fn refresh_sliders(
    settings: Res<MachineSettings>,
    menus: Query<&Menu>,
    mut labels: Query<(&SliderLabel, &mut TextColor)>,
    mut fills: Query<(&SliderFill, &mut Node, &mut BackgroundColor)>,
    mut values: Query<(&SliderValue, &mut Text, &mut TextColor), Without<SliderLabel>>,
) {
    let Ok(menu) = menus.single() else { return };
    let color_at = |index: usize| {
        if index == menu.active {
            ACTIVE_COLOR
        } else {
            INACTIVE_COLOR
        }
    };
    for (label, mut color) in &mut labels {
        if color.0 != color_at(label.0) {
            color.0 = color_at(label.0);
        }
    }
    for (fill, mut node, mut background) in &mut fills {
        let width = Val::Percent(kind(fill.0).get(&settings.volume) * 100.0);
        if node.width != width {
            node.width = width;
        }
        if background.0 != color_at(fill.0) {
            background.0 = color_at(fill.0);
        }
    }
    for (value, mut text, mut color) in &mut values {
        let percent = format!("{:.0}%", kind(value.0).get(&settings.volume) * 100.0);
        if text.0 != percent {
            text.0 = percent;
        }
        if color.0 != color_at(value.0) {
            color.0 = color_at(value.0);
        }
    }
}

fn handle_cancel(actions: Actions, mut fade: ResMut<SceneFade>, mut sfx: MessageWriter<PlaySfx>) {
    if actions.just_pressed(GameAction::cancel(PlayerId::P1)) {
        sfx.write(PlaySfx(Sfx::Cancel));
        fade.begin(GameScene::SettingsMenu);
    }
}
