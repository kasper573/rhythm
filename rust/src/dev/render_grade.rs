//! Renders every grade text to a PNG so the grade shader can be tuned
//! without playing. Each grade gets a row at peak glow, shown twice — on
//! black (left) and on a playfield-like gray (right); it reuses the real
//! grade-text rig and shader so what it shows is exactly what the game
//! draws.

use crate::core::config::{GameConfig, RowOutcome, config};
use crate::core::font::{TextPivot, label, place_label};
use crate::core::screen::CLEAR_COLOR;
use crate::core::units::Seconds;
use crate::game::Game;
use crate::nodes::stepfile_player::grade_text::{apply_style, grade_style, spawn_rig};
use godot::classes::window::ContentScaleMode;
use godot::classes::{ColorRect, Control, INode, Node, Node2D};
use godot::prelude::*;
use std::path::PathBuf;

const WIDTH: i32 = 900;
const HEIGHT: i32 = 760;
/// The two columns' word centers, from the canvas center.
const COLUMN_X: [f32; 2] = [-210.0, 210.0];
/// Frames to let the offscreen words render before capturing.
const SETTLE_FRAMES: u32 = 30;

pub(super) fn start(game: &mut Game, out: PathBuf) {
    let mut window = game.base().get_window().expect("the game runs in a window");
    window.set_content_scale_mode(ContentScaleMode::DISABLED);
    window.set_size(Vector2i::new(WIDTH, HEIGHT));

    // The host enters the tree before any text is placed: label metrics
    // only resolve inside the tree.
    let mut host = Control::new_alloc();
    host.set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);
    game.base_mut().add_child(&host);
    let mut backdrop = ColorRect::new_alloc();
    backdrop.set_color(CLEAR_COLOR);
    backdrop.set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);
    host.add_child(&backdrop);

    // Two backgrounds so the glow can be judged for equal visibility on
    // black and on a lighter playfield-like gray, not just pure black.
    for (column, color) in [Color::BLACK, Color::from_rgb(0.30, 0.31, 0.35)]
        .into_iter()
        .enumerate()
    {
        let mut half = ColorRect::new_alloc();
        half.set_color(color);
        half.set_position(Vector2::new(column as f32 * WIDTH as f32 / 2.0, 0.0));
        half.set_size(Vector2::new(WIDTH as f32 / 2.0, HEIGHT as f32));
        host.add_child(&half);
    }

    let mut canvas = Node2D::new_alloc();
    canvas.set_position(Vector2::new(WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0));
    host.add_child(&canvas);
    for (column, text) in ["on black", "on gray"].into_iter().enumerate() {
        let mut caption = label(text, 24.0, Color::from_rgb(0.6, 0.6, 0.6));
        canvas.add_child(&caption);
        place_label(
            &mut caption,
            Vector2::new(COLUMN_X[column], -(HEIGHT as f32 / 2.0 - 26.0)),
            TextPivot::CENTER,
        );
    }

    // A representative outcome for each dynamic grade — the midpoint of its
    // timing window, so it grades to exactly that tier — followed by a miss.
    let outcomes = grade_outcomes(config());
    let row_gap = (HEIGHT as f32 - 90.0) / outcomes.len() as f32;
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

    let mut driver = RenderGradeDriver::new_alloc();
    driver.bind_mut().out = out;
    game.base_mut().add_child(&driver);
}

#[derive(GodotClass)]
#[class(base=Node)]
struct RenderGradeDriver {
    frames: u32,
    out: PathBuf,
    base: Base<Node>,
}

#[godot_api]
impl INode for RenderGradeDriver {
    fn init(base: Base<Node>) -> RenderGradeDriver {
        RenderGradeDriver {
            frames: 0,
            out: PathBuf::from("out"),
            base,
        }
    }

    fn process(&mut self, _delta: f64) {
        self.frames += 1;
        if self.frames < SETTLE_FRAMES {
            return;
        }
        let path = self.out.join("grades.png");
        std::fs::create_dir_all(&self.out).expect("failed to create the output directory");
        let viewport = self
            .base()
            .get_viewport()
            .expect("the driver lives in the tree");
        let texture = viewport.get_texture().expect("the viewport renders");
        let image = texture.get_image().expect("the viewport has an image");
        image.save_png(&path.display().to_string());
        println!("wrote {}", path.display());
        self.base().get_tree().quit();
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
