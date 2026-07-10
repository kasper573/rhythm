//! The judgment word that pops on each graded row, shaded live: the word
//! is rendered white into an offscreen viewport so its alpha is pure
//! coverage, then presented on a sprite whose material tints it to the
//! grade color and layers on an additive glow that pulses — one shader
//! (the colocated `grade_text.gdshader`, embedded in the binary),
//! per-grade colors and strengths.

use crate::core::config::{DynamicGradeDef, GameConfig, Grade, RowOutcome, TimingFeedback};
use crate::core::font::{TextPivot, label, place_label};
use crate::core::player::PlayerId;
use crate::core::units::{Percent, Seconds};
use godot::classes::{Label, Node2D, Shader, ShaderMaterial, Sprite2D, SubViewport};
use godot::prelude::*;

/// The word's font height, in canvas units.
const FONT_SIZE: f32 = 50.0;
/// Canvas size of the presented sprite; the offscreen viewport frames
/// exactly this, wide enough to hold the longest timing-feedback string.
pub const PRESENT_W: f32 = 760.0;
pub const PRESENT_H: f32 = 170.0;
/// The offscreen word renders larger than presented so it stays crisp
/// when the window scales the sprite up.
const SUPERSAMPLE: f32 = 2.0;
/// The neon glow's reach around the glyphs, in canvas units, and its
/// brightness.
const HALO_RADIUS: f32 = 18.0;
const HALO_STRENGTH: f32 = 1.6;
/// The glow strikes to full the instant a grade lands, then drains toward
/// [`GLOW_FLOOR`] with this time constant — a per-hit pulse, not a steady
/// shine, in the spirit of the wheel and health-bar pulses.
const PULSE_TAU: f32 = 0.3;
const GLOW_FLOOR: f32 = 0.32;
/// The grade group's vertical extent, keeping the word and the combo under
/// it inside the screen's padded band.
const GRADE_HALF_HEIGHT: f32 = 36.0;
const COMBO_HALF_HEIGHT: f32 = 24.0;
/// The combo readout sits this far under the grade's center.
pub const COMBO_GAP: f32 = 62.0;
/// The pop each grade opens with: a brief upscale that settles.
const BOUNCE_SECONDS: f32 = 0.13;
const BOUNCE_AMOUNT: f32 = 0.18;
/// Seconds a grade takes to fade out once the player stops hitting.
const FADE_SECONDS: f32 = 1.0;

/// The pieces of one grade-text rig: the offscreen white word and the
/// shader sprite presenting it. Owners position and re-layer the sprite;
/// freeing it frees the whole rig (the viewport rides along as its child).
pub struct GradeRig {
    pub sprite: Gd<Sprite2D>,
    label: Gd<Label>,
    pub material: Gd<ShaderMaterial>,
}

/// Builds a grade-text rig under `layer`, presenting at the layer-local
/// origin until positioned. Shared by the live sessions and the
/// `render_grade` inspector.
pub fn spawn_rig(layer: &mut Node2D) -> GradeRig {
    let mut viewport = SubViewport::new_alloc();
    viewport.set_transparent_background(true);
    viewport.set_size(Vector2i::new(
        (PRESENT_W * SUPERSAMPLE) as i32,
        (PRESENT_H * SUPERSAMPLE) as i32,
    ));
    viewport.set_update_mode(godot::classes::sub_viewport::UpdateMode::ALWAYS);

    let mut word = label("", FONT_SIZE * SUPERSAMPLE, Color::WHITE);
    viewport.add_child(&word);
    place_label(
        &mut word,
        Vector2::new(PRESENT_W, PRESENT_H) * SUPERSAMPLE / 2.0,
        TextPivot::CENTER,
    );

    let mut shader = Shader::new_gd();
    shader.set_code(include_str!("grade_text.gdshader"));
    let mut material = ShaderMaterial::new_gd();
    material.set_shader(&shader);
    material.set_shader_parameter(
        "shape",
        &Vector4::new(
            HALO_RADIUS / PRESENT_W,
            HALO_RADIUS / PRESENT_H,
            HALO_STRENGTH,
            0.0,
        )
        .to_variant(),
    );

    let mut sprite = Sprite2D::new_alloc();
    sprite.add_child(&viewport);
    if let Some(texture) = viewport.get_texture() {
        sprite.set_texture(&texture);
    }
    sprite.set_scale(Vector2::splat(1.0 / SUPERSAMPLE));
    sprite.set_material(&material);
    layer.add_child(&sprite);

    GradeRig {
        sprite,
        label: word,
        material,
    }
}

impl GradeRig {
    pub fn set_text(&mut self, text: &str) {
        self.label.set_text(text);
        place_label(
            &mut self.label,
            Vector2::new(PRESENT_W, PRESENT_H) * SUPERSAMPLE / 2.0,
            TextPivot::CENTER,
        );
    }

    pub fn free(&mut self) {
        self.sprite.queue_free();
    }
}

/// One player's grade word on stage, driven by the session: refreshed on
/// each graded row, animated every frame.
pub struct GradeDisplay {
    pub player: PlayerId,
    pub rig: GradeRig,
    origin_x: f32,
    /// Fade level: 1 on a fresh grade, decaying to 0 once hits stop.
    intensity: f32,
    /// Seconds since the last grade, driving the glow's oscillation.
    pulse: f32,
    /// The pop-in bounce, 1 on a fresh grade decaying to 0.
    bounce: f32,
    /// The current grade's color, glow color, and glow strength.
    base: Color,
    glow: Color,
    strength: f32,
}

impl GradeDisplay {
    pub fn new(layer: &mut Node2D, player: PlayerId, origin_x: f32) -> GradeDisplay {
        let mut rig = spawn_rig(layer);
        rig.sprite.set_position(Vector2::new(origin_x, 0.0));
        rig.sprite.set_visible(false);
        GradeDisplay {
            player,
            rig,
            origin_x,
            intensity: 0.0,
            pulse: 0.0,
            bounce: 0.0,
            base: Color::WHITE,
            glow: Color::WHITE,
            strength: 0.0,
        }
    }

    /// Follows a field refit.
    pub fn set_origin_x(&mut self, origin_x: f32) {
        self.origin_x = origin_x;
    }

    /// A graded row refreshes the word, color, and glow, and restarts the
    /// pop and fade.
    pub fn apply(&mut self, config: &GameConfig, outcome: RowOutcome) {
        let style = grade_style(config, outcome);
        self.rig.set_text(&style.text);
        self.base = style.base;
        self.glow = style.glow;
        self.strength = style.strength;
        self.intensity = 1.0;
        self.pulse = 0.0;
        self.bounce = 1.0;
        self.rig.sprite.set_visible(true);
    }

    /// Advances the word's fade, glow pulse, and pop, and keeps it at `y`
    /// (the player's configured height, layer-local y-up).
    pub fn animate(&mut self, delta: f32, y: f32) {
        if self.intensity <= 0.0 {
            return;
        }
        self.intensity = (self.intensity - delta / FADE_SECONDS).max(0.0);
        self.pulse += delta;
        self.bounce = (self.bounce - delta / BOUNCE_SECONDS).max(0.0);

        apply_style(
            &mut self.rig.material,
            self.base,
            self.glow,
            self.strength,
            self.intensity,
            glow_pulse(self.pulse),
        );

        let bounce = ease_cubic_out(self.bounce);
        self.rig
            .sprite
            .set_scale(Vector2::splat((1.0 + BOUNCE_AMOUNT * bounce) / SUPERSAMPLE));
        self.rig
            .sprite
            .set_position(Vector2::new(self.origin_x, -y));
    }
}

/// The canvas Y band the grade group occupies (centered y-up coordinates),
/// top (0%) to bottom (100%). The session's owner fills it: the play stage
/// from the padded window, the options preview from its modal stripe.
#[derive(Default, Clone, Copy)]
pub struct GradeArea {
    pub top: f32,
    pub bottom: f32,
}

/// The grade word's y for a player's grade-position percentage within its
/// area: 0% at the top, 100% at the bottom.
pub fn grade_y(area: &GradeArea, grade_position: Percent) -> f32 {
    let t = (grade_position.0 / 100.0).clamp(0.0, 1.0);
    area.top + (area.bottom - area.top) * t
}

/// The grade area for a usable band spanning `top_edge`..`bottom_edge`
/// (centered y-up), inset so the word and the combo tracking under it both
/// stay inside.
pub fn grade_area(top_edge: f32, bottom_edge: f32) -> GradeArea {
    GradeArea {
        top: top_edge - GRADE_HALF_HEIGHT,
        bottom: bottom_edge + COMBO_GAP + COMBO_HALF_HEIGHT,
    }
}

/// Packs a grade's colors into the shader uniforms at a given fade and glow
/// pulse — the one place the color/glow packing lives, shared by the live
/// animation and the `render_grade` inspector.
pub fn apply_style(
    material: &mut Gd<ShaderMaterial>,
    base: Color,
    glow: Color,
    strength: f32,
    intensity: f32,
    pulse: f32,
) {
    let base = Vector4::new(base.r, base.g, base.b, intensity);
    let glow = Vector4::new(glow.r, glow.g, glow.b, strength * pulse);
    material.set_shader_parameter("base_color", &base.to_variant());
    material.set_shader_parameter("glow_color", &glow.to_variant());
}

/// The glow strength at a moment since the grade landed: full at the strike
/// (`seconds` 0), draining toward [`GLOW_FLOOR`].
pub fn glow_pulse(seconds: f32) -> f32 {
    GLOW_FLOOR + (1.0 - GLOW_FLOOR) * (-seconds / PULSE_TAU).exp()
}

/// The word, base color, glow color, and glow strength one outcome shows.
pub struct GradeStyle {
    pub text: String,
    pub base: Color,
    pub glow: Color,
    pub strength: f32,
}

pub fn grade_style(config: &GameConfig, outcome: RowOutcome) -> GradeStyle {
    match outcome {
        RowOutcome::Hit { error } => {
            let Grade::Hit(grade) = config.grade(outcome) else {
                unreachable!("hits always grade into a timed grade");
            };
            let def = &config.grading.dynamic[grade.0];
            // Like ITG: the letters are white, the grade's color is the glow.
            GradeStyle {
                text: hit_text(def, error),
                base: Color::WHITE,
                glow: def.glow.color,
                strength: def.glow.strength,
            }
        }
        RowOutcome::Miss => {
            // ITG's Miss is the exception — its letters carry the red.
            let miss = &config.grading.fixed.miss;
            GradeStyle {
                text: miss.name.clone(),
                base: miss.color,
                glow: miss.glow.color,
                strength: miss.glow.strength,
            }
        }
    }
}

/// The word for a hit, marking the side of the perfect moment the input
/// fell on: early feedback leads the name, late feedback trails it.
fn hit_text(def: &DynamicGradeDef, error: Seconds) -> String {
    let name = &def.name;
    let early = error.0 > 0.0;
    // Displayed offset is input-relative: negative = early, positive = late.
    let offset_ms = (-error.to_millis()).round() as i64;
    match def.timing_feedback {
        TimingFeedback::Off => name.clone(),
        TimingFeedback::Sign if early => format!("-{name}"),
        TimingFeedback::Sign => format!("{name}-"),
        TimingFeedback::Millis if early => format!("({offset_ms}ms) {name}"),
        TimingFeedback::Millis => format!("{name} (+{offset_ms}ms)"),
    }
}

fn ease_cubic_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}
