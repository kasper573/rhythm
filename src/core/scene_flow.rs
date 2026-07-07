use bevy::prelude::*;

/// Every scene in the game. Scene systems run under `in_state`, scene
/// entities carry `DespawnOnExit(GameScene::...)`.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameScene {
    #[default]
    MainMenu,
    SettingsMenu,
    Keymap,
    FileSelect,
    PlayerOptions,
    FilePlayer,
    Score,
}

/// Drives the mandatory scene transition: fade to black, swap scene while
/// black, fade back in. All scene switches must go through [`SceneFade::begin`].
#[derive(Resource)]
pub struct SceneFade {
    phase: FadePhase,
}

impl SceneFade {
    pub fn begin(&mut self, to: GameScene) {
        if matches!(self.phase, FadePhase::FadingOut { .. }) {
            return;
        }
        let alpha = self.alpha();
        self.phase = FadePhase::FadingOut { to, alpha };
    }

    /// Whether scenes should react to input right now. Input is ignored while
    /// fading out to avoid acting on a scene that is already on its way out.
    pub fn accepts_input(&self) -> bool {
        !matches!(self.phase, FadePhase::FadingOut { .. })
    }

    fn alpha(&self) -> f32 {
        match self.phase {
            FadePhase::Idle => 0.0,
            FadePhase::FadingOut { alpha, .. } | FadePhase::FadingIn { alpha } => alpha,
        }
    }
}

/// Run condition: the current scene is fully faded in and accepting input.
pub fn scene_accepts_input(fade: Res<SceneFade>) -> bool {
    fade.accepts_input()
}

pub struct SceneFlowPlugin;

impl Plugin for SceneFlowPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameScene>()
            // Boot behind a fully black overlay that fades in like any other
            // scene entrance.
            .insert_resource(SceneFade {
                phase: FadePhase::FadingIn { alpha: 1.0 },
            })
            .add_systems(Startup, spawn_fade_overlay)
            .add_systems(Update, run_fade);
    }
}

const FADE_SECONDS: f32 = 0.3;

#[derive(Clone, Copy)]
enum FadePhase {
    Idle,
    FadingOut { to: GameScene, alpha: f32 },
    FadingIn { alpha: f32 },
}

#[derive(Component)]
struct FadeOverlay;

fn spawn_fade_overlay(mut commands: Commands) {
    commands.spawn((
        FadeOverlay,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 1.0)),
        GlobalZIndex(1000),
    ));
}

fn run_fade(
    time: Res<Time>,
    mut fade: ResMut<SceneFade>,
    mut next_scene: ResMut<NextState<GameScene>>,
    mut overlay: Single<&mut BackgroundColor, With<FadeOverlay>>,
) {
    if matches!(fade.phase, FadePhase::Idle) {
        return;
    }
    let step = time.delta_secs() / FADE_SECONDS;
    fade.phase = match fade.phase {
        FadePhase::Idle => FadePhase::Idle,
        FadePhase::FadingOut { to, alpha } => {
            let alpha = alpha + step;
            if alpha >= 1.0 {
                // Swap scenes while the screen is fully black.
                next_scene.set(to);
                FadePhase::FadingIn { alpha: 1.0 }
            } else {
                FadePhase::FadingOut { to, alpha }
            }
        }
        FadePhase::FadingIn { alpha } => {
            let alpha = alpha - step;
            if alpha <= 0.0 {
                FadePhase::Idle
            } else {
                FadePhase::FadingIn { alpha }
            }
        }
    };
    overlay.0.set_alpha(fade.alpha());
}
