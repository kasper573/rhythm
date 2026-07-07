use crate::core::config::{GameConfig, Judgment};
use crate::core::font::GameFont;
use crate::core::input::{Actions, GameAction};
use crate::core::menu::TITLE_COLOR;
use crate::core::sfx::{PlaySfx, Sfx};
use crate::scenes::file_player::ScoreResults;
use crate::scenes::file_select::FileSelectTarget;
use crate::scenes::{GameScene, SceneFade, scene_accepts_input};
use bevy::prelude::*;

pub struct ScoreScenePlugin;

impl Plugin for ScoreScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameScene::Score), enter)
            .add_systems(OnExit(GameScene::Score), exit)
            .add_systems(
                Update,
                leave.run_if(in_state(GameScene::Score).and_then(scene_accepts_input)),
            );
    }
}

fn exit(mut commands: Commands) {
    commands.remove_resource::<ScoreResults>();
}

fn enter(
    mut commands: Commands,
    results: Option<Res<ScoreResults>>,
    config: Res<GameConfig>,
    font: Res<GameFont>,
    mut fade: ResMut<SceneFade>,
) {
    let Some(results) = results else {
        fade.begin(GameScene::FileSelect);
        return;
    };

    // Grades are derived from the raw outcomes, so score and gameplay can
    // never disagree about what a timing error means.
    let mut grade_counts = vec![0u32; config.grades.len()];
    let mut miss_count = 0u32;
    for outcome in &results.outcomes {
        match config.judge(*outcome) {
            Judgment::Grade(grade) => grade_counts[grade.0] += 1,
            Judgment::Miss => miss_count += 1,
        }
    }

    commands
        .spawn((
            DespawnOnExit(GameScene::Score),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(8.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(results.title.clone()),
                font.sized(46.0),
                TextColor(TITLE_COLOR),
                Node {
                    margin: UiRect::bottom(Val::Px(28.0)),
                    ..default()
                },
            ));
            // Best to worst (config validates smallest window first), Miss last.
            for (grade, count) in config.grades.iter().zip(&grade_counts) {
                parent.spawn((
                    Text::new(format!("{:<12} {count}", grade.name)),
                    font.sized(30.0),
                    TextColor(grade.color),
                ));
            }
            parent.spawn((
                Text::new(format!("{:<12} {miss_count}", config.miss_appearance.name)),
                font.sized(30.0),
                TextColor(config.miss_appearance.color),
            ));
            parent.spawn((
                Text::new(format!(
                    "{:<12} {}/{}",
                    "Holds", results.holds_ok, results.holds_total
                )),
                font.sized(30.0),
                TextColor(Color::srgb(0.8, 0.85, 0.8)),
            ));
            parent.spawn((
                Text::new(format!(
                    "{:<12} {}/{} avoided",
                    "Mines",
                    results.mines_total - results.mines_hit,
                    results.mines_total
                )),
                font.sized(30.0),
                TextColor(Color::srgb(0.8, 0.85, 0.8)),
            ));
            parent.spawn((
                Text::new(format!("Max combo: {}", results.max_combo)),
                font.sized(32.0),
                TextColor(Color::srgb(0.7, 0.85, 1.0)),
                Node {
                    margin: UiRect::top(Val::Px(24.0)),
                    ..default()
                },
            ));
        });
}

fn leave(
    actions: Actions,
    results: Res<ScoreResults>,
    mut commands: Commands,
    mut fade: ResMut<SceneFade>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let sound = if actions.just_pressed(GameAction::Select) {
        Sfx::Select
    } else if actions.just_pressed(GameAction::Cancel) {
        Sfx::Cancel
    } else {
        return;
    };
    sfx.write(PlaySfx(sound));
    commands.insert_resource(FileSelectTarget::Stepfile(results.id));
    fade.begin(GameScene::FileSelect);
}
