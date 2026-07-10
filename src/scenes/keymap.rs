use crate::core::config::GameConfig;
use crate::core::font::game_font;
use crate::core::input::GameAction;
use crate::core::scene_flow::SpawnScoped;
use crate::core::settings::MachineSettings;
use crate::core::sfx::{PlaySfx, Sfx};
use crate::core::units::Seconds;
use crate::prefabs::menu::{INACTIVE_COLOR, Menu, MenuItem, OwnerDrivenMenu, TITLE_COLOR};
use crate::scenes::{
    GameScene, SceneFade, play_default_bgm, scene_accepts_input, spawn_default_background,
};
use bevy::ecs::query::QueryFilter;
use bevy::ecs::system::SystemParam;
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use strum::{EnumCount, IntoEnumIterator};

/// resolve through the config's DEFAULT keymap, never the live one it edits:
/// however broken the stored bindings get, this scene stays operable to
/// repair them.
pub(super) struct KeymapScenePlugin;

impl Plugin for KeymapScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(GameScene::Keymap),
            (enter, play_default_bgm, spawn_default_background),
        )
        .add_systems(OnExit(GameScene::Keymap), exit)
        // The scene's own systems act only while no prompt is open and run
        // before the capture, so the key that closes a prompt can never
        // double as navigation or scene exit in the same frame.
        .add_systems(
            Update,
            (
                (navigate_rows, open_prompt, cancel_gesture)
                    .chain()
                    .distributive_run_if(prompt_closed),
                capture_prompt_key,
                refresh_rows,
            )
                .chain()
                .run_if(in_state(GameScene::Keymap).and_then(scene_accepts_input)),
        );
    }
}

/// The scene's own controls act only between prompts; outside the scene
/// the resource is gone and they stay off.
fn prompt_closed(prompt: Option<Res<Prompt>>) -> bool {
    prompt.is_some_and(|prompt| prompt.action.is_none())
}

/// The scene's fixed input: raw key state resolved through the config's
/// default keymap (see [`default_key`]), with [`Actions`](crate::core::input::Actions)'
/// vocabulary.
#[derive(SystemParam)]
struct DefaultKeys<'w> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    config: Res<'w, GameConfig>,
}

impl DefaultKeys<'_> {
    fn just_pressed(&self, action: GameAction) -> bool {
        self.keys.just_pressed(default_key(&self.config, action))
    }

    fn pressed(&self, action: GameAction) -> bool {
        self.keys.pressed(default_key(&self.config, action))
    }

    fn just_released(&self, action: GameAction) -> bool {
        self.keys.just_released(default_key(&self.config, action))
    }
}

/// The rebind prompt: which action we are listening for, and whether the
/// prompt was opened this frame (whose key events must be ignored so the
/// ¤P1Select¤ press that opened it doesn't bind itself).
#[derive(Resource, Default)]
struct Prompt {
    action: Option<GameAction>,
    just_opened: bool,
}

#[derive(Component, Default, Clone)]
struct PromptLabel;

/// Marks the key column's text of an action row (the row's index rides
/// its `MenuItem`), refreshed on rebinds.
#[derive(Component, Default, Clone)]
struct BindingText;

#[derive(QueryFilter)]
struct BindingCell {
    _binding: With<BindingText>,
    _not_prompt: Without<PromptLabel>,
}

/// The table is centered by fixed positioning inside a box of these
/// dimensions, never by flex centering: content sits within the box, so
/// neither the prompt appearing nor a binding's width changing can shift it.
const TABLE_WIDTH: f32 = 560.0;
const TABLE_HEIGHT: f32 = 620.0;

/// A fixed key column keeps the grid's own width constant as bindings change
/// length, so the centering above never recomputes.
const KEY_COLUMN_WIDTH: f32 = 200.0;

/// The key column's trailing slack (short keys don't fill it) pulls the
/// visible ink left of the box's true center; a positive left inset against
/// the auto margins nudges the box right by half this to rebalance it.
const CENTER_BIAS: f32 = 130.0;

fn enter(mut commands: Commands, settings: Res<MachineSettings>, config: Res<GameConfig>) {
    commands.init_resource::<Prompt>();
    // A column-major grid of left-aligned action/key pairs: the first
    // splice fills the label column, the second the key column, and both
    // cells of a row carry its `MenuItem` so the highlight covers it whole.
    let labels: Vec<_> = GameAction::iter()
        .enumerate()
        .map(|(index, action)| {
            bsn! {
                MenuItem(index)
                game_font(19.0)
                Text({action.label().to_string()})
                TextColor({INACTIVE_COLOR})
            }
        })
        .collect();
    let keys: Vec<_> = GameAction::iter()
        .enumerate()
        .map(|(index, action)| {
            let key = key_label(action, &settings, &config);
            bsn! {
                MenuItem(index)
                BindingText
                game_font(19.0)
                Text({key})
                TextColor({INACTIVE_COLOR})
            }
        })
        .collect();
    let reset_help = format!(
        "Hold {:?} to reset selected key to default",
        default_key(&config, GameAction::P1Cancel)
    );
    commands.spawn_scoped(
        GameScene::Keymap,
        bsn! {
            Node { width: percent(100), height: percent(100) }
            Children [
                // Fixed-size box centered by auto margins, not flex centering:
                // its position is immune to what its content does.
                (
                    Node {
                        position_type: PositionType::Absolute,
                        left: {Val::Px(CENTER_BIAS)},
                        right: px(0),
                        top: px(0),
                        bottom: px(0),
                        margin: {UiRect::all(Val::Auto)},
                        width: {Val::Px(TABLE_WIDTH)},
                        height: {Val::Px(TABLE_HEIGHT)},
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        row_gap: px(8),
                    }
                    Children [
                        (
                            game_font(44.0)
                            Text("Keymap")
                            TextColor({TITLE_COLOR})
                            Node { margin: {UiRect::bottom(Val::Px(12.0))} }
                        ),
                        (
                            Menu { active: 0, len: {GameAction::COUNT} }
                            OwnerDrivenMenu
                            Node {
                                display: Display::Grid,
                                grid_auto_flow: GridAutoFlow::Column,
                                grid_template_rows: {vec![RepeatedGridTrack::auto(GameAction::COUNT as u16)]},
                                grid_template_columns: {vec![RepeatedGridTrack::auto(1), RepeatedGridTrack::px(1, KEY_COLUMN_WIDTH)]},
                                justify_items: JustifyItems::Start,
                                column_gap: px(48),
                                row_gap: px(2),
                            }
                            Children [ {labels}, {keys} ]
                        ),
                    ]
                ),
                // Out of the table's flow: a viewport-anchored line whose text
                // may grow without disturbing the table above it.
                (
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        right: px(0),
                        bottom: px(56),
                        justify_content: JustifyContent::Center,
                    }
                    Children [(
                        PromptLabel
                        game_font(22.0)
                        Text("")
                        TextColor(Color::srgb(0.4, 0.9, 0.6))
                    )]
                ),
                (
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        right: px(0),
                        bottom: px(16),
                        justify_content: JustifyContent::Center,
                    }
                    Children [(
                        game_font(18.0)
                        Text({reset_help})
                        TextColor({INACTIVE_COLOR})
                    )]
                ),
            ]
        },
    );
}

fn exit(mut commands: Commands) {
    commands.remove_resource::<Prompt>();
}

/// The scene's fixed resolution: an action's key in the config's default
/// keymap, untouched by the live overrides this scene edits.
fn default_key(config: &GameConfig, action: GameAction) -> KeyCode {
    config
        .defaults
        .keymap
        .binding(action)
        .expect("validated: defaults.keymap binds every action")
}

fn navigate_rows(input: DefaultKeys, mut menus: Query<&mut Menu>, mut sfx: MessageWriter<PlaySfx>) {
    let step_back = match (
        input.just_pressed(GameAction::P1Up),
        input.just_pressed(GameAction::P1Down),
    ) {
        (true, false) => true,
        (false, true) => false,
        _ => return,
    };
    for mut menu in &mut menus {
        menu.active = if step_back {
            (menu.active + menu.len - 1) % menu.len
        } else {
            (menu.active + 1) % menu.len
        };
        sfx.write(PlaySfx(Sfx::Navigate));
    }
}

fn open_prompt(
    input: DefaultKeys,
    menus: Query<&Menu>,
    mut prompt: ResMut<Prompt>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if !input.just_pressed(GameAction::P1Select) {
        return;
    }
    for menu in &menus {
        prompt.action = GameAction::iter().nth(menu.active);
        prompt.just_opened = true;
        sfx.write(PlaySfx(Sfx::Select));
    }
}

/// How long ¤P1Cancel¤ must be held to reset instead of leave.
const RESET_HOLD: Seconds = Seconds(0.5);

/// The ¤P1Cancel¤ hold state; only presses that began while no prompt was
/// open are armed, so the press that aborts a prompt neither taps nor
/// resets.
#[derive(Default)]
struct CancelHold {
    held: Seconds,
    armed: bool,
}

/// The ¤P1Cancel¤ gesture: holding resets the active row's binding to its
/// default, a shorter tap leaves the scene.
fn cancel_gesture(
    input: DefaultKeys,
    time: Res<Time>,
    menus: Query<&Menu>,
    mut hold: Local<CancelHold>,
    mut settings: ResMut<MachineSettings>,
    mut fade: ResMut<SceneFade>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if input.just_pressed(GameAction::P1Cancel) {
        hold.armed = true;
        hold.held = Seconds::ZERO;
    }
    if !hold.armed {
        return;
    }
    if input.pressed(GameAction::P1Cancel) {
        hold.held += Seconds(time.delta_secs_f64());
        if hold.held >= RESET_HOLD {
            hold.armed = false;
            for menu in &menus {
                let Some(action) = GameAction::iter().nth(menu.active) else {
                    continue;
                };
                settings.keymap.reset(action);
                sfx.write(PlaySfx(Sfx::Select));
            }
        }
        return;
    }
    if input.just_released(GameAction::P1Cancel) {
        hold.armed = false;
        sfx.write(PlaySfx(Sfx::Cancel));
        fade.begin(GameScene::SettingsMenu);
    }
}

fn capture_prompt_key(
    config: Res<GameConfig>,
    mut prompt: ResMut<Prompt>,
    mut keyboard: MessageReader<KeyboardInput>,
    mut settings: ResMut<MachineSettings>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let Some(action) = prompt.action else {
        keyboard.clear();
        return;
    };
    if prompt.just_opened {
        prompt.just_opened = false;
        keyboard.clear();
        return;
    }
    for event in keyboard.read() {
        if event.state != ButtonState::Pressed || event.repeat {
            continue;
        }
        if event.key_code == default_key(&config, GameAction::P1Cancel) {
            sfx.write(PlaySfx(Sfx::Cancel));
        } else {
            settings.keymap.set(action, event.key_code);
            sfx.write(PlaySfx(Sfx::Select));
        }
        prompt.action = None;
        break;
    }
}

fn refresh_rows(
    settings: Res<MachineSettings>,
    config: Res<GameConfig>,
    prompt: Res<Prompt>,
    mut bindings: Query<(&MenuItem, &mut Text), BindingCell>,
    mut prompt_label: Single<&mut Text, With<PromptLabel>>,
) {
    if settings.is_changed() {
        for (item, mut text) in &mut bindings {
            let Some(action) = GameAction::iter().nth(item.0) else {
                continue;
            };
            text.0 = key_label(action, &settings, &config);
        }
    }
    if prompt.is_changed() {
        prompt_label.0 = match prompt.action {
            Some(action) => format!(
                "Press a key for \"{}\" ({:?} aborts)",
                action.label(),
                default_key(&config, GameAction::P1Cancel)
            ),
            None => String::new(),
        };
    }
}

fn key_label(action: GameAction, settings: &MachineSettings, config: &GameConfig) -> String {
    format!("{:?}", settings.keymap.key(action, config))
}
