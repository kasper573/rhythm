use bevy::prelude::*;
use bevy::state::state::FreelyMutableState;
use std::marker::PhantomData;

/// Drives the mandatory scene transition for the scene state `S`: fade to
/// black, swap scene while black, fade back in. All scene switches must go
/// through [`SceneFade::begin`].
#[derive(Resource)]
pub struct SceneFade<S: FreelyMutableState> {
    phase: FadePhase<S>,
}

impl<S: FreelyMutableState> SceneFade<S> {
    pub fn begin(&mut self, to: S) {
        if matches!(self.phase, FadePhase::FadingOut { .. }) {
            return;
        }
        let alpha = self.alpha();
        self.phase = FadePhase::FadingOut { to, alpha };
    }

    /// Input is ignored while fading out, to avoid acting on a scene that is
    /// already on its way out.
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

pub struct SceneFlowPlugin<S>(PhantomData<S>);

impl<S> Default for SceneFlowPlugin<S> {
    fn default() -> Self {
        SceneFlowPlugin(PhantomData)
    }
}

impl<S: FreelyMutableState + FromWorld> Plugin for SceneFlowPlugin<S> {
    fn build(&self, app: &mut App) {
        app.init_state::<S>()
            // Boot behind a fully black overlay that fades in like any other
            // scene entrance.
            .insert_resource(SceneFade::<S> {
                phase: FadePhase::FadingIn { alpha: 1.0 },
            })
            .add_systems(Startup, spawn_fade_overlay)
            .add_systems(Update, run_fade::<S>);
    }
}

const FADE_SECONDS: f32 = 0.3;

#[derive(Clone)]
enum FadePhase<S> {
    Idle,
    FadingOut { to: S, alpha: f32 },
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

fn run_fade<S: FreelyMutableState>(
    time: Res<Time>,
    mut fade: ResMut<SceneFade<S>>,
    mut next_scene: ResMut<NextState<S>>,
    mut overlay: Single<&mut BackgroundColor, With<FadeOverlay>>,
) {
    if matches!(fade.phase, FadePhase::Idle) {
        return;
    }
    let step = time.delta_secs() / FADE_SECONDS;
    fade.phase = match fade.phase.clone() {
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
