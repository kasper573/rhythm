use crate::core::config::{GameConfig, Grade, GradeIndex};
use crate::core::font::game_font;
use crate::core::high_scores::{HighScores, highscore_key};
use crate::core::input::{Actions, GameAction};
use crate::core::library::{StepfileId, StepfileLibrary};
use crate::core::player::PlayerId;
use crate::core::scene_flow::SpawnScoped;
use crate::core::sfx::{PlaySfx, Sfx};
use crate::core::units::Percent;
use crate::prefabs::menu::TITLE_COLOR;
use crate::prefabs::stepfile_player::StageResults;
use crate::scenes::wheel::WheelTarget;
use crate::scenes::{
    GameScene, SceneFade, play_default_bgm, scene_accepts_input, spawn_default_background,
};
use bevy::prelude::*;

/// The score scene's entry param: a finished session's results, inserted
/// by the play scene (or the bench), consumed on enter.
#[derive(Resource, Debug, Clone)]
pub struct ScoreResults {
    pub id: StepfileId,
    pub title: String,
    pub players: Vec<PlayerResult>,
}

/// One player's complete run: the chart they played and its results.
#[derive(Debug, Clone)]
pub struct PlayerResult {
    /// Index into the played stepfile's `charts`.
    pub chart: usize,
    pub stage: StageResults,
}

pub(super) struct ScoreScenePlugin;

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
        fade.begin(GameScene::Wheel);
        return;
    };

    // One column per player, in player order: P1 left, P2 right.
    let tagged = results.players.len() > 1;
    let columns: Vec<_> = results
        .players
        .iter()
        .map(|player| {
            let chart = &library.stepfile(results.id).stepfile.charts[player.chart];
            let key = highscore_key(&library, results.id, chart);
            let tally = tally(&config, &player.stage);
            let new_high_score = high_scores.record(player.stage.player, key, tally.total_points);
            player_column(
                &config,
                &asset_server,
                &player.stage,
                &tally,
                new_high_score,
                tagged,
            )
        })
        .collect();

    let title = results.title.clone();
    commands.spawn_scoped(
        GameScene::Score,
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
                    game_font(46.0)
                    Text({title})
                    TextColor({TITLE_COLOR})
                ),
                (
                    Node {
                        column_gap: px(120),
                        align_items: AlignItems::FlexStart,
                    }
                    Children [ {columns} ]
                ),
            ]
        },
    );
}

/// One player's full result column: their outcome, score, tallies, and
/// combo, tagged with their slot when both players show.
fn player_column(
    config: &GameConfig,
    asset_server: &AssetServer,
    stage: &StageResults,
    tally: &Tally,
    new_high_score: bool,
    tagged: bool,
) -> impl Scene + use<> {
    // Best to worst (config validates smallest window first), Miss last.
    let mut lines: Vec<(String, String, Color)> = config
        .grading
        .dynamic
        .iter()
        .zip(&tally.grade_counts)
        .map(|(grade, count)| (grade.name.clone(), count.to_string(), grade.color))
        .collect();
    lines.push((
        config.grading.fixed.miss.name.clone(),
        tally.miss_count.to_string(),
        config.grading.fixed.miss.color,
    ));
    lines.push((
        "Holds".to_string(),
        format!("{}/{}", stage.holds_ok, stage.holds_total),
        Color::srgb(0.8, 0.85, 0.8),
    ));
    lines.push((
        "Mines".to_string(),
        format!(
            "{}/{}",
            stage.mines_total - stage.mines_exploded,
            stage.mines_total
        ),
        Color::srgb(0.8, 0.85, 0.8),
    ));
    // Two left-aligned columns; rows line up because every cell shares one
    // font size and line height.
    let (labels, values): (Vec<_>, Vec<_>) = lines
        .into_iter()
        .map(|(label, value, color)| {
            (
                bsn! {
                    game_font(30.0)
                    Text({label})
                    TextColor({color})
                },
                bsn! {
                    game_font(30.0)
                    Text({value})
                    TextColor({color})
                },
            )
        })
        .unzip();
    let tallies = bsn! {
        Node { column_gap: px(28) }
        Children [
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    row_gap: px(2),
                }
                Children [ {labels} ]
            ),
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    row_gap: px(2),
                }
                Children [ {values} ]
            ),
        ]
    };

    let header: Vec<_> = tagged
        .then(|| {
            let tag = stage.player.label().to_string();
            bsn! {
                game_font(36.0)
                Text({tag})
                TextColor({TITLE_COLOR})
            }
        })
        .into_iter()
        .collect();

    let (result_label, result_color) = if stage.failed {
        ("FAILED", Color::srgb(0.95, 0.25, 0.25))
    } else {
        ("CLEARED", Color::srgb(0.5, 0.95, 0.6))
    };
    let result_line = bsn! {
        game_font(34.0)
        Text({result_label.to_string()})
        TextColor({result_color})
        Node { margin: {UiRect::bottom(Val::Px(12.0))} }
    };

    let rating_image = asset_server.load(
        config
            .rating(tally.percent, tally.worst_grade)
            .image
            .clone(),
    );
    // One unwrapped line overlaid on the rating art, centered on it and
    // hugging its bottom edge; wider text simply overflows both sides.
    let overlay: Vec<_> = new_high_score
        .then(|| {
            bsn! {
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    right: px(0),
                    bottom: px(-4),
                    justify_content: JustifyContent::Center,
                }
                Children [(
                    game_font(16.0)
                    Text("New high score!")
                    TextLayout { linebreak: LineBreak::NoWrap }
                    TextColor(Color::srgb(1.0, 0.85, 0.35))
                )]
            }
        })
        .into_iter()
        .collect();
    let percent_line = tally.percent.to_string();
    let score_line = bsn! {
        Node {
            column_gap: px(16),
            align_items: AlignItems::Center,
            margin: {UiRect::bottom(Val::Px(10.0))},
        }
        Children [
            (
                game_font(42.0)
                Text({percent_line})
                TextColor(Color::srgb(0.95, 0.97, 1.0))
            ),
            (
                Node { height: px(56) }
                Children [
                    (
                        ImageNode { image: {rating_image} }
                        Node { height: percent(100) }
                    ),
                    {overlay},
                ]
            ),
        ]
    };

    let combo = format!("Max combo: {}", stage.max_combo);
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(8),
        }
        Children [
            {header},
            {result_line},
            {score_line},
            {tallies},
            (
                game_font(32.0)
                Text({combo})
                TextColor(Color::srgb(0.7, 0.85, 1.0))
                Node { margin: {UiRect::top(Val::Px(16.0))} }
            ),
        ]
    }
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

fn tally(config: &GameConfig, stage: &StageResults) -> Tally {
    let mut grade_counts = vec![0u32; config.grading.dynamic.len()];
    let mut miss_count = 0u32;
    for outcome in &stage.outcomes {
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
        + stage.holds_ok * config.grading.fixed.ok.points
        + stage.holds_ng * config.grading.fixed.ng.points;

    let complete = stage.outcomes.len() as u32 == stage.rows_total;
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
        percent: config.score_percent(total_points, stage.rows_total, stage.holds_total),
        grade_counts,
        miss_count,
        total_points,
        worst_grade,
    }
}

/// Any player who just played may leave the results; the wheel returns to
/// the played stepfile.
fn leave(
    actions: Actions,
    results: Res<ScoreResults>,
    mut commands: Commands,
    mut fade: ResMut<SceneFade>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let pressed = |action: fn(PlayerId) -> GameAction| {
        results
            .players
            .iter()
            .any(|player: &PlayerResult| actions.just_pressed(action(player.stage.player)))
    };
    let sound = if pressed(GameAction::select) {
        Sfx::Select
    } else if pressed(GameAction::cancel) {
        Sfx::Cancel
    } else {
        return;
    };
    sfx.write(PlaySfx(sound));
    commands.insert_resource(WheelTarget(results.id));
    fade.begin(GameScene::Wheel);
}
