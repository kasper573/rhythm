use bevy::prelude::*;
use bevy::state::state::FreelyMutableState;
use std::marker::PhantomData;

/// Drives the mandatory scene transition for the scene state `S`: fade to
/// black, swap scene while black, fade back in. All scene switches must go
/// through [`SceneFade::begin`].
#[derive(Resource)]
pub struct SceneFade<S: FreelyMutableState> {
    phase: FadePhase<S>,
    alpha: f32,
}

impl<S: FreelyMutableState> SceneFade<S> {
    pub fn begin(&mut self, to: S) {
        if !matches!(self.phase, FadePhase::FadingOut(_)) {
            self.phase = FadePhase::FadingOut(to);
        }
    }

    /// Input is ignored while fading out, to avoid acting on a scene that is
    /// already on its way out.
    pub fn accepts_input(&self) -> bool {
        !matches!(self.phase, FadePhase::FadingOut(_))
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
                phase: FadePhase::FadingIn,
                alpha: 1.0,
            })
            .add_systems(Startup, spawn_fade_overlay)
            .add_systems(Update, run_fade::<S>);
    }
}

const FADE_SECONDS: f32 = 0.3;

#[derive(Clone)]
enum FadePhase<S> {
    Idle,
    FadingOut(S),
    FadingIn,
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
    let step = time.delta_secs() / FADE_SECONDS;
    match fade.phase.clone() {
        FadePhase::Idle => return,
        FadePhase::FadingOut(to) => {
            fade.alpha = (fade.alpha + step).min(1.0);
            if fade.alpha >= 1.0 {
                // Swap scenes while the screen is fully black.
                next_scene.set(to);
                fade.phase = FadePhase::FadingIn;
            }
        }
        FadePhase::FadingIn => {
            fade.alpha = (fade.alpha - step).max(0.0);
            if fade.alpha <= 0.0 {
                fade.phase = FadePhase::Idle;
            }
        }
    }
    overlay.0.set_alpha(fade.alpha);
}
