use crate::core::config::{GameConfig, Grade, GradeIndex};
use crate::core::font::game_font;
use crate::core::high_scores::{HighScores, highscore_key};
use crate::core::input::{Actions, GameAction};
use crate::core::library::StepfileLibrary;
use crate::core::menu::TITLE_COLOR;
use crate::core::scene_flow::SpawnScoped;
use crate::core::sfx::{PlaySfx, Sfx};
use crate::core::units::Percent;
use crate::scenes::file_player::{PlayResult, ScoreResults};
use crate::scenes::file_select::FileSelectTarget;
use crate::scenes::{
    GameScene, SceneFade, play_default_bgm, scene_accepts_input, spawn_default_background,
};
use bevy::prelude::*;

pub struct ScoreScenePlugin;

impl Plugin for ScoreScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(GameScene::Score),
            (enter, play_default_bgm, spawn_default_background),
        )
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
    library: Res<StepfileLibrary>,
    mut high_scores: ResMut<HighScores>,
    asset_server: Res<AssetServer>,
    mut fade: ResMut<SceneFade>,
) {
    let Some(results) = results else {
        fade.begin(GameScene::FileSelect);
        return;
    };

    let tally = tally(&config, &results);

    // Best to worst (config validates smallest window first), Miss last.
    let mut tallies: Vec<_> = config
        .grading
        .dynamic
        .iter()
        .zip(&tally.grade_counts)
        .map(|(grade, count)| (format!("{:<12} {count}", grade.name), grade.color))
        .collect();
    tallies.push((
        format!(
            "{:<12} {}",
            config.grading.fixed.miss.name, tally.miss_count
        ),
        config.grading.fixed.miss.color,
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
            results.mines_total - results.mines_exploded,
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

    let rating_image = asset_server.load(
        config
            .rating(tally.percent, tally.worst_grade)
            .image
            .clone(),
    );

    let key = highscore_key(
        library.group_name(results.id),
        &library.stepfile(results.id).name(),
        &results.difficulty,
    );
    let new_high_score = high_scores.record(key, tally.total_points);
    let overlay: Vec<_> = new_high_score
        .then(|| {
            bsn! {
                game_font(18.0)
                Text("New high score!")
                TextColor(Color::srgb(1.0, 0.85, 0.35))
                Node {
                    position_type: PositionType::Absolute,
                    top: px(-14),
                    right: px(-30),
                }
            }
        })
        .into_iter()
        .collect();
    let percent_line = tally.percent.to_string();
    let score_line = bsn! {
        Node {
            column_gap: px(16),
            align_items: AlignItems::Center,
            margin: {UiRect::bottom(Val::Px(14.0))},
        }
        Children [
            (
                game_font(42.0)
                Text({percent_line})
                TextColor(Color::srgb(0.95, 0.97, 1.0))
            ),
            (
                ImageNode { image: {rating_image} }
                Node { height: px(56) }
            ),
            {overlay},
        ]
    };

    let title = results.title.clone();
    let (result_label, result_color) = match results.result {
        PlayResult::Cleared => ("CLEARED", Color::srgb(0.5, 0.95, 0.6)),
        PlayResult::Failed => ("FAILED", Color::srgb(0.95, 0.25, 0.25)),
    };
    let result_line = bsn! {
        game_font(34.0)
        Text({result_label.to_string()})
        TextColor({result_color})
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
                {result_line},
                {score_line},
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

/// Everything the score derives from the raw outcomes — grades and score
/// are always recomputed from them, so score and gameplay can never
/// disagree about what a timing error means.
struct Tally {
    /// Per dynamic grade, in config order.
    grade_counts: Vec<u32>,
    miss_count: u32,
    total_points: u32,
    /// Of a perfect run over the whole chart, holds included.
    percent: Percent,
    /// The worst grade any row earned; `None` when part of the chart went
    /// ungraded (failed runs), which no all-grades rating rule matches.
    worst_grade: Option<Grade>,
}

fn tally(config: &GameConfig, results: &ScoreResults) -> Tally {
    let mut grade_counts = vec![0u32; config.grading.dynamic.len()];
    let mut miss_count = 0u32;
    for outcome in &results.outcomes {
        match config.grade(*outcome) {
            Grade::Hit(grade) => grade_counts[grade.0] += 1,
            Grade::Miss => miss_count += 1,
        }
    }

    let total_points = grade_counts
        .iter()
        .zip(&config.grading.dynamic)
        .map(|(count, grade)| count * grade.points)
        .sum::<u32>()
        + miss_count * config.grading.fixed.miss.points
        + results.holds_ok * config.grading.fixed.ok.points
        + results.holds_ng * config.grading.fixed.ng.points;

    let complete = results.outcomes.len() as u32 == results.rows_total;
    let worst_grade = complete.then(|| {
        if miss_count > 0 {
            Grade::Miss
        } else {
            Grade::Hit(GradeIndex(
                grade_counts
                    .iter()
                    .rposition(|count| *count > 0)
                    .unwrap_or(0),
            ))
        }
    });

    Tally {
        percent: config.score_percent(total_points, results.rows_total, results.holds_total),
        grade_counts,
        miss_count,
        total_points,
        worst_grade,
    }
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
