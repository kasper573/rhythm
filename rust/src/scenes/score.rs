use crate::core::assets::asset_root;
use crate::core::config::{GameConfig, Grade, GradeIndex, config};
use crate::core::font::label;
use crate::core::high_scores::{HighScores, highscore_key};
use crate::core::input::{Actions, GameAction};
use crate::core::library::{StepfileId, library};
use crate::core::player::PlayerId;
use crate::core::screen::TITLE_COLOR;
use crate::core::sfx::Sfx;
use crate::core::textures::PendingTexture;
use crate::core::units::Percent;
use crate::game::Game;
use crate::nodes::stepfile_player::StageResults;
use crate::scenes::{
    GameScene, change_scene, play_default_bgm, scene_accepts_input, spawn_default_background,
};
use godot::classes::control::{LayoutPreset, SizeFlags};
use godot::classes::texture_rect::ExpandMode;
use godot::classes::{
    CenterContainer, Control, HBoxContainer, IControl, MarginContainer, TextureRect, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;

/// A finished session's results, inserted by the play scene (or the bench)
/// as the score scene's entry param; consumed on enter.
#[derive(Debug, Clone)]
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

#[derive(GodotClass)]
#[class(base=Control)]
pub struct ScoreScene {
    players: Vec<PlayerId>,
    id: Option<StepfileId>,
    ratings: Vec<(PendingTexture, Gd<TextureRect>)>,
    base: Base<Control>,
}

#[godot_api]
impl ScoreScene {
    pub fn instantiate(game: &mut Game) -> Gd<ScoreScene> {
        let mut scene = ScoreScene::new_alloc();
        scene.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        let Some(results) = game.take_score_results() else {
            game.change_scene(GameScene::Wheel);
            return scene;
        };
        play_default_bgm();
        spawn_default_background(&mut scene.clone().upcast::<Control>());

        let mut center = CenterContainer::new_alloc();
        center.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        let mut column = VBoxContainer::new_alloc();
        column.add_theme_constant_override("separation", 20);
        column.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
        let mut title = label(&results.title, 46.0, TITLE_COLOR);
        title.set_horizontal_alignment(HorizontalAlignment::CENTER);
        title.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        column.add_child(&title);

        let mut columns_row = HBoxContainer::new_alloc();
        columns_row.add_theme_constant_override("separation", 120);
        columns_row.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        let tagged = results.players.len() > 1;
        let mut bound = scene.bind_mut();
        bound.id = Some(results.id);
        for player in &results.players {
            let chart = &library().stepfile(results.id).stepfile.charts[player.chart];
            let key = highscore_key(library(), results.id, chart);
            let tally = tally(config(), &player.stage);
            let new_high_score = HighScores::singleton().bind_mut().record(
                player.stage.player,
                key,
                tally.total_points,
            );
            let column_node =
                bound.player_column(config(), &player.stage, &tally, new_high_score, tagged);
            columns_row.add_child(&column_node);
            bound.players.push(player.stage.player);
        }
        drop(bound);
        column.add_child(&columns_row);
        center.add_child(&column);
        scene.add_child(&center);
        scene
    }

    /// One player's full result column: their outcome, score, tallies, and
    /// combo, tagged with their slot when both players show.
    fn player_column(
        &mut self,
        config: &GameConfig,
        stage: &StageResults,
        tally: &Tally,
        new_high_score: bool,
        tagged: bool,
    ) -> Gd<VBoxContainer> {
        let mut column = VBoxContainer::new_alloc();
        column.add_theme_constant_override("separation", 8);
        column.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);

        if tagged {
            let mut tag = label(stage.player.label(), 36.0, TITLE_COLOR);
            tag.set_horizontal_alignment(HorizontalAlignment::CENTER);
            tag.set_h_size_flags(SizeFlags::SHRINK_CENTER);
            column.add_child(&tag);
        }

        let (result_label, result_color) = if stage.failed {
            ("FAILED", Color::from_rgb(0.95, 0.25, 0.25))
        } else {
            ("CLEARED", Color::from_rgb(0.5, 0.95, 0.6))
        };
        let mut result = label(result_label, 34.0, result_color);
        result.set_horizontal_alignment(HorizontalAlignment::CENTER);
        let mut result_box = MarginContainer::new_alloc();
        result_box.add_theme_constant_override("margin_bottom", 12);
        result_box.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        result_box.add_child(&result);
        column.add_child(&result_box);

        // The percentage beside the rating art, which carries a "new high
        // score" ribbon on its bottom edge when earned.
        let mut score_row = HBoxContainer::new_alloc();
        score_row.add_theme_constant_override("separation", 16);
        score_row.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        let percent = label(
            &tally.percent.to_string(),
            42.0,
            Color::from_rgb(0.95, 0.97, 1.0),
        );
        score_row.add_child(&percent);
        let mut rating_box = Control::new_alloc();
        rating_box.set_custom_minimum_size(Vector2::new(56.0, 56.0));
        let mut rating = TextureRect::new_alloc();
        rating.set_expand_mode(ExpandMode::IGNORE_SIZE);
        rating.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT);
        rating.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        rating_box.add_child(&rating);
        if new_high_score {
            let mut ribbon = label("New high score!", 16.0, Color::from_rgb(1.0, 0.85, 0.35));
            ribbon.set_horizontal_alignment(HorizontalAlignment::CENTER);
            ribbon.set_anchors_and_offsets_preset(LayoutPreset::BOTTOM_WIDE);
            ribbon.set_offset(godot::builtin::Side::TOP, -4.0);
            ribbon.set_offset(godot::builtin::Side::BOTTOM, 20.0);
            rating_box.add_child(&ribbon);
        }
        score_row.add_child(&rating_box);
        let mut score_box = MarginContainer::new_alloc();
        score_box.add_theme_constant_override("margin_bottom", 10);
        score_box.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        score_box.add_child(&score_row);
        column.add_child(&score_box);
        let image = config
            .rating(tally.percent, tally.worst_grade)
            .image
            .clone();
        self.ratings
            .push((PendingTexture::load(asset_root().join(image)), rating));

        // Best to worst (config validates smallest window first), Miss
        // last; then the holds and mines kept.
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
            Color::from_rgb(0.8, 0.85, 0.8),
        ));
        lines.push((
            "Mines".to_string(),
            format!(
                "{}/{}",
                stage.mines_total - stage.mines_exploded,
                stage.mines_total
            ),
            Color::from_rgb(0.8, 0.85, 0.8),
        ));
        let mut tallies = HBoxContainer::new_alloc();
        tallies.add_theme_constant_override("separation", 28);
        tallies.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        let mut labels_column = VBoxContainer::new_alloc();
        labels_column.add_theme_constant_override("separation", 2);
        let mut values_column = VBoxContainer::new_alloc();
        values_column.add_theme_constant_override("separation", 2);
        for (name, value, color) in lines {
            labels_column.add_child(&label(&name, 30.0, color));
            values_column.add_child(&label(&value, 30.0, color));
        }
        tallies.add_child(&labels_column);
        tallies.add_child(&values_column);
        column.add_child(&tallies);

        let mut combo_gap = Control::new_alloc();
        combo_gap.set_custom_minimum_size(Vector2::new(0.0, 8.0));
        column.add_child(&combo_gap);
        let mut combo = label(
            &format!("Max combo: {}", stage.max_combo),
            32.0,
            Color::from_rgb(0.7, 0.85, 1.0),
        );
        combo.set_horizontal_alignment(HorizontalAlignment::CENTER);
        combo.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        column.add_child(&combo);
        column
    }
}

#[godot_api]
impl IControl for ScoreScene {
    fn init(base: Base<Control>) -> ScoreScene {
        ScoreScene {
            players: Vec::new(),
            id: None,
            ratings: Vec::new(),
            base,
        }
    }

    fn process(&mut self, _delta: f64) {
        self.ratings.retain_mut(|(pending, target)| {
            let Some(result) = pending.poll() else {
                return true;
            };
            if let Some(texture) = result {
                target.set_texture(&texture);
            }
            false
        });
        if !scene_accepts_input() {
            return;
        }
        // Any player who just played may leave the results; the wheel
        // returns to the played stepfile.
        let pressed = |action: fn(PlayerId) -> GameAction| {
            self.players
                .iter()
                .any(|player| Actions::just_pressed(action(*player)))
        };
        let sound = if pressed(GameAction::select) {
            Sfx::Select
        } else if pressed(GameAction::cancel) {
            Sfx::Cancel
        } else {
            return;
        };
        sound.play();
        if let Some(id) = self.id {
            Game::singleton().bind_mut().set_wheel_target(id);
        }
        change_scene(GameScene::Wheel);
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
