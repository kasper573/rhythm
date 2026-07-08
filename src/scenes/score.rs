use crate::core::config::{GameConfig, Judgment};
use crate::core::font::game_font;
use crate::core::input::{Actions, GameAction};
use crate::core::menu::TITLE_COLOR;
use crate::core::scene_flow::SpawnScoped;
use crate::core::sfx::{PlaySfx, Sfx};
use crate::scenes::file_player::{PlayResult, ScoreResults};
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

    // Best to worst (config validates smallest window first), Miss last.
    let mut tallies: Vec<_> = config
        .grades
        .iter()
        .zip(&grade_counts)
        .map(|(grade, count)| (format!("{:<12} {count}", grade.name), grade.color))
        .collect();
    tallies.push((
        format!("{:<12} {miss_count}", config.miss.name),
        config.miss.color,
    ));
    tallies.push((
        format!(
            "{:<12} {}/{}",
            "Holds", results.holds_ok, results.holds_total
        ),
        Color::srgb(0.8, 0.85, 0.8),
    ));
    tallies.push((
        format!(
            "{:<12} {}/{} avoided",
            "Mines",
            results.mines_total - results.mines_hit,
            results.mines_total
        ),
        Color::srgb(0.8, 0.85, 0.8),
    ));
    let tallies: Vec<_> = tallies
        .into_iter()
        .map(|(line, color)| {
            bsn! {
                game_font(30.0)
                Text({line})
                TextColor({color})
            }
        })
        .collect();

    let title = results.title.clone();
    let (verdict_label, verdict_color) = match results.result {
        PlayResult::Cleared => ("CLEARED", Color::srgb(0.5, 0.95, 0.6)),
        PlayResult::Failed => ("FAILED", Color::srgb(0.95, 0.25, 0.25)),
    };
    let verdict = bsn! {
        game_font(34.0)
        Text({verdict_label.to_string()})
        TextColor({verdict_color})
        Node { margin: {UiRect::bottom(Val::Px(20.0))} }
    };
    let combo = format!("Max combo: {}", results.max_combo);
    commands.spawn_scoped(
        GameScene::Score,
        bsn! {
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(8),
            }
            Children [
                (
                    game_font(46.0)
                    Text({title})
                    TextColor({TITLE_COLOR})
                    Node { margin: {UiRect::bottom(Val::Px(4.0))} }
                ),
                {verdict},
                {tallies},
                (
                    game_font(32.0)
                    Text({combo})
                    TextColor(Color::srgb(0.7, 0.85, 1.0))
                    Node { margin: {UiRect::top(Val::Px(24.0))} }
                ),
            ]
        },
    );
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
