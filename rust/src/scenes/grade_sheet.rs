//! The grade sheet: every grade text at peak glow, on black and on a
//! playfield-like gray side by side, so the grade shader can be reviewed
//! and tuned without playing. Deep-linked with `--scene grade-sheet`.

use crate::core::config::{GameConfig, RowOutcome, config};
use crate::core::font::{TextPivot, label, place_label};
use crate::core::screen::CLEAR_COLOR;
use crate::core::units::Seconds;
use crate::game::Game;
use crate::nodes::stepfile_player::grade_text::{apply_style, grade_style, spawn_rig};
use godot::classes::control::LayoutPreset;
use godot::classes::window::ContentScaleMode;
use godot::classes::{ColorRect, Control, IControl, Node2D};
use godot::prelude::*;

/// The two columns' word centers, from the canvas center.
const COLUMN_X: [f32; 2] = [-210.0, 210.0];

#[derive(GodotClass)]
#[class(init, base=Control)]
pub struct GradeSheetScene {
    base: Base<Control>,
}

#[godot_api]
impl GradeSheetScene {
    /// The sheet draws 1:1 at whatever size the window was launched with
    /// (the tooling picks its capture resolution with `--resolution`).
    pub fn instantiate(game: &mut Game) -> Gd<GradeSheetScene> {
        let mut window = game.base().get_window().expect("the game runs in a window");
        window.set_content_scale_mode(ContentScaleMode::DISABLED);
        let size = window.get_size();

        let mut scene = GradeSheetScene::new_alloc();
        scene.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        let mut backdrop = ColorRect::new_alloc();
        backdrop.set_color(CLEAR_COLOR);
        backdrop.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        scene.add_child(&backdrop);

        // Two backgrounds so the glow can be judged for equal visibility on
        // black and on a lighter playfield-like gray, not just pure black.
        for (column, color) in [Color::BLACK, Color::from_rgb(0.30, 0.31, 0.35)]
            .into_iter()
            .enumerate()
        {
            let mut half = ColorRect::new_alloc();
            half.set_color(color);
            half.set_position(Vector2::new(column as f32 * size.x as f32 / 2.0, 0.0));
            half.set_size(Vector2::new(size.x as f32 / 2.0, size.y as f32));
            scene.add_child(&half);
        }
        scene
    }
}

#[godot_api]
impl IControl for GradeSheetScene {
    /// The words are laid out from inside the tree: their labels only
    /// measure once they are part of it.
    fn ready(&mut self) {
        let size = self
            .base()
            .get_window()
            .expect("the game runs in a window")
            .get_size();
        let mut canvas = Node2D::new_alloc();
        canvas.set_position(Vector2::new(size.x as f32 / 2.0, size.y as f32 / 2.0));
        self.base_mut().add_child(&canvas);
        for (column, text) in ["on black", "on gray"].into_iter().enumerate() {
            let mut caption = label(text, 24.0, Color::from_rgb(0.6, 0.6, 0.6));
            canvas.add_child(&caption);
            place_label(
                &mut caption,
                Vector2::new(COLUMN_X[column], -(size.y as f32 / 2.0 - 26.0)),
                TextPivot::CENTER,
            );
        }

        let outcomes = grade_outcomes(config());
        let row_gap = (size.y as f32 - 90.0) / outcomes.len() as f32;
        let top = (outcomes.len() as f32 - 1.0) / 2.0 * row_gap;
        for (row, outcome) in outcomes.iter().enumerate() {
            let style = grade_style(config(), *outcome);
            let y = top - row as f32 * row_gap;
            for x in COLUMN_X {
                let mut rig = spawn_rig(&mut canvas);
                rig.set_text(&style.text);
                apply_style(
                    &mut rig.material,
                    style.base,
                    style.glow,
                    style.strength,
                    1.0,
                    1.0,
                );
                rig.sprite.set_position(Vector2::new(x, -y));
            }
        }
    }
}

/// A representative outcome for each dynamic grade — the midpoint of its
/// timing window, so it grades to exactly that tier — followed by a miss.
fn grade_outcomes(config: &GameConfig) -> Vec<RowOutcome> {
    let mut outcomes = Vec::new();
    let mut lower = Seconds::ZERO;
    for grade in &config.grading.dynamic {
        outcomes.push(RowOutcome::Hit {
            error: Seconds((lower.0 + grade.window.0) / 2.0),
        });
        lower = grade.window;
    }
    outcomes.push(RowOutcome::Miss);
    outcomes
}
