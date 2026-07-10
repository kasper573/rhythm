use crate::core::font::label;
use crate::core::input::{Actions, GameAction};
use godot::classes::control::LayoutPreset;
use godot::classes::{ColorRect, Control, IControl, Label, Shader, ShaderMaterial, VBoxContainer};
use godot::prelude::*;

pub struct FpsOverlayOptions {
    /// Bar and readout color.
    pub fg: Color,
    /// Panel and graph backdrop.
    pub bg: Color,
    /// Distance from the bottom-right screen corner.
    pub edge_padding: f32,
}

/// A frame-rate meter pinned to the bottom-right corner: the current FPS and
/// its observed range as text above a scrolling histogram of recent frames.
/// Hidden until the ¤Toggle FPS¤ action shows it; the node listens for the
/// toggle itself.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct FpsOverlay {
    history: FpsHistory,
    smoothed: f32,
    readout: Option<Gd<Label>>,
    graph: Option<Gd<ShaderMaterial>>,
    base: Base<Control>,
}

#[godot_api]
impl FpsOverlay {
    pub fn instantiate(opt: FpsOverlayOptions) -> Gd<FpsOverlay> {
        let mut overlay = FpsOverlay::new_alloc();
        overlay.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        overlay.set_visible(false);
        overlay.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);

        let mut panel = ColorRect::new_alloc();
        panel.set_color(opt.bg);
        let mut column = VBoxContainer::new_alloc();
        column.add_theme_constant_override("separation", 3);

        let mut readout = label("", READOUT_SIZE, opt.fg);
        column.add_child(&readout);
        readout.set_text("");

        let mut shader = Shader::new_gd();
        shader.set_code(include_str!("fps_overlay.gdshader"));
        let mut material = ShaderMaterial::new_gd();
        material.set_shader(&shader);
        material.set_shader_parameter("fg", &opt.fg.to_variant());
        material.set_shader_parameter("bg", &opt.bg.to_variant());
        let mut graph = ColorRect::new_alloc();
        graph.set_custom_minimum_size(Vector2::new(GRAPH_WIDTH, GRAPH_HEIGHT));
        graph.set_material(&material);
        column.add_child(&graph);

        panel.add_child(&column);
        column.set_position(Vector2::new(PANEL_PADDING, PANEL_PADDING));
        overlay.add_child(&panel);

        let panel_size = Vector2::new(
            GRAPH_WIDTH + PANEL_PADDING * 2.0,
            READOUT_SIZE + 8.0 + GRAPH_HEIGHT + PANEL_PADDING * 2.0,
        );
        panel.set_anchors_preset(LayoutPreset::BOTTOM_RIGHT);
        panel.set_offset(godot::builtin::Side::LEFT, -panel_size.x - opt.edge_padding);
        panel.set_offset(godot::builtin::Side::TOP, -panel_size.y - opt.edge_padding);
        panel.set_offset(godot::builtin::Side::RIGHT, -opt.edge_padding);
        panel.set_offset(godot::builtin::Side::BOTTOM, -opt.edge_padding);
        {
            let mut bound = overlay.bind_mut();
            bound.readout = Some(readout);
            bound.graph = Some(material);
        }
        overlay
    }
}

#[godot_api]
impl IControl for FpsOverlay {
    fn init(base: Base<Control>) -> FpsOverlay {
        FpsOverlay {
            history: FpsHistory::default(),
            smoothed: 0.0,
            readout: None,
            graph: None,
            base,
        }
    }

    fn process(&mut self, delta: f64) {
        if Actions::just_pressed(GameAction::ToggleFps) {
            let visible = self.base().is_visible();
            self.base_mut().set_visible(!visible);
        }
        if !self.base().is_visible() || delta <= 0.0 {
            return;
        }
        let fps = (1.0 / delta) as f32;
        // Exponential smoothing keeps the readout legible at high rates
        // while the graph shows every raw frame.
        self.smoothed += (fps - self.smoothed) * if self.smoothed == 0.0 { 1.0 } else { 0.1 };
        self.history.push(fps);
        let (low, high) = self.history.range().unwrap_or((fps, fps));
        let smoothed = self.smoothed;
        if let Some(readout) = &mut self.readout {
            readout.set_text(&format!("{smoothed:.0} FPS ({low:.0}-{high:.0})"));
        }
        if let Some(graph) = &mut self.graph {
            let samples = PackedFloat32Array::from(self.history.normalized().as_slice());
            graph.set_shader_parameter("samples", &samples.to_variant());
        }
    }
}

const READOUT_SIZE: f32 = 13.0;
const PANEL_PADDING: f32 = 4.0;
const GRAPH_WIDTH: f32 = 120.0;
const GRAPH_HEIGHT: f32 = 34.0;
const COLUMNS: usize = 96;

/// A ring of the most recent per-frame FPS readings, feeding both the graph's
/// bars and the readout's min/max.
struct FpsHistory {
    ring: [f32; COLUMNS],
    next: usize,
}

impl Default for FpsHistory {
    fn default() -> FpsHistory {
        FpsHistory {
            ring: [0.0; COLUMNS],
            next: 0,
        }
    }
}

impl FpsHistory {
    fn push(&mut self, fps: f32) {
        self.ring[self.next] = fps;
        self.next = (self.next + 1) % COLUMNS;
    }

    /// The samples oldest-to-newest, each normalized to the window's peak so
    /// the tallest recent frame fills the graph.
    fn normalized(&self) -> [f32; COLUMNS] {
        let peak = self.ring.iter().copied().fold(0.0, f32::max).max(1.0);
        let mut samples = [0.0; COLUMNS];
        for (column, sample) in samples.iter_mut().enumerate() {
            *sample = (self.ring[(self.next + column) % COLUMNS] / peak).clamp(0.0, 1.0);
        }
        samples
    }

    /// The `(min, max)` over the frames observed so far, or `None` before the
    /// first frame.
    fn range(&self) -> Option<(f32, f32)> {
        let mut observed = self.ring.iter().copied().filter(|fps| *fps > 0.0);
        let first = observed.next()?;
        Some(observed.fold((first, first), |(low, high), fps| {
            (low.min(fps), high.max(fps))
        }))
    }
}
