use super::{
    BANNER_SIZE, BANNER_TEXT, BANNER_TINT, BPM_TEXT, DETAILS_BOX_CENTER, DETAILS_BOX_SIZE,
    STATS_TEXT, WheelEntry, WheelScene,
};
use crate::core::font::{TextPivot, label, place_label};
use crate::core::library::library;
use crate::core::stepfile::{Difficulty, DisplayBpm, Stepfile};
use crate::core::textures::PendingTexture;
use crate::core::units::Seconds;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{Node2D, TextureRect};
use godot::prelude::*;

impl WheelScene {
    /// Rebuilt from scratch on each settle: despawn-and-respawn beats
    /// mutating a panel of text nodes in place.
    pub(super) fn refresh_info_panel(&mut self) {
        if !self.just_settled {
            return;
        }
        if let Some(mut panel) = self.info_panel.take() {
            panel.queue_free();
        }
        self.banner = None;
        let Some(entry) = self.entries.get(self.active).copied() else {
            return;
        };

        let (banner_path, fallback_title, headline, charts) = match entry {
            WheelEntry::Stepfile { id } => {
                let entry = library().stepfile(id);
                let charts: Vec<_> = self
                    .players
                    .clone()
                    .into_iter()
                    .filter_map(|player| {
                        self.chart_for(&entry.stepfile, player)
                            .map(|index| (player, id, index))
                    })
                    .collect();
                (
                    entry.banner_path(),
                    entry.display_title(),
                    bpm_label(&entry.stepfile),
                    charts,
                )
            }
            WheelEntry::Group { index } => {
                let group = &library().groups[index];
                let headline = match group.stepfiles.len() {
                    1 => "1 stepfile".to_string(),
                    count => format!("{count} stepfiles"),
                };
                (
                    group.banner_path.clone(),
                    group.name.clone(),
                    headline,
                    Vec::new(),
                )
            }
        };

        // Rows without a banner of their own fall back to the default BGM's.
        let banner_path = banner_path.or_else(|| library().default_bgm.banner_path());

        let mut panel = Node2D::new_alloc();
        panel.set_position(Vector2::new(-320.0, 0.0));
        panel.set_z_index(50);

        // Real banners cover the fixed rect like a CSS `object-fit: cover`
        // image — scaled to fill, centered, overflow cropped, never
        // stretched. The generated placeholder is made to stretch.
        let banner_y = DETAILS_BOX_CENTER.y + (DETAILS_BOX_SIZE.y - BANNER_SIZE.y) / 2.0;
        let mut banner = TextureRect::new_alloc();
        banner.set_size(BANNER_SIZE);
        banner.set_position(Vector2::new(
            -BANNER_SIZE.x / 2.0,
            -banner_y - BANNER_SIZE.y / 2.0,
        ));
        banner.set_expand_mode(ExpandMode::IGNORE_SIZE);
        banner.set_clip_contents(true);
        panel.add_child(&banner);
        match banner_path {
            Some(path) => {
                banner.set_stretch_mode(StretchMode::KEEP_ASPECT_COVERED);
                self.banner = Some((PendingTexture::load(path), banner));
            }
            None => {
                banner.set_stretch_mode(StretchMode::SCALE);
                banner.set_texture(&self.bar_texture);
                banner.set_modulate(BANNER_TINT);
                let mut title = label(&fallback_title, 24.0, BANNER_TEXT);
                panel.add_child(&title);
                place_label(&mut title, Vector2::new(0.0, -banner_y), TextPivot::CENTER);
                title.set_z_index(1);
            }
        }

        let mut headline_label = label(&headline, 28.0, BPM_TEXT);
        panel.add_child(&headline_label);
        place_label(
            &mut headline_label,
            Vector2::new(0.0, -70.0),
            TextPivot::CENTER,
        );

        // One difficulty line per active player (tagged when there are
        // two), with the chart stats below in single-player modes; versus
        // swaps the stats for the second player's line.
        let tagged = charts.len() > 1;
        for (row, (player, id, index)) in charts.iter().enumerate() {
            let stepfile = &library().stepfile(*id).stepfile;
            let chart = &stepfile.charts[*index];
            let (name, color) = difficulty_style(&chart.difficulty);
            let line = if tagged {
                format!("{}  {name} {}", player.label(), chart.meter)
            } else {
                format!("{name} {}", chart.meter)
            };
            let mut chart_line = label(&line, 34.0, color);
            panel.add_child(&chart_line);
            place_label(
                &mut chart_line,
                Vector2::new(0.0, -(18.0 - row as f32 * 42.0)),
                TextPivot::CENTER,
            );
            if !tagged {
                for (text, x, y) in stat_grid(stepfile, *index) {
                    let mut cell = label(&text, 22.0, STATS_TEXT);
                    panel.add_child(&cell);
                    place_label(&mut cell, Vector2::new(x, -y), TextPivot::CENTER_LEFT);
                }
            }
        }

        if let Some(canvas) = &mut self.canvas {
            canvas.add_child(&panel);
        }
        self.info_panel = Some(panel);
    }

    /// Resolves the banner texture once its bytes arrive.
    pub(super) fn poll_banner(&mut self) {
        let Some((pending, target)) = &mut self.banner else {
            return;
        };
        let Some(result) = pending.poll() else {
            return;
        };
        if let Some(texture) = result {
            target.set_texture(&texture);
        }
        self.banner = None;
    }
}

/// Horizontal starts of the stat grid's label and value columns, two
/// pairs side by side, in panel-local coordinates.
const STAT_COLUMNS: [(f32, f32); 2] = [(-170.0, -75.0), (35.0, 130.0)];
const STAT_TOP_Y: f32 = -48.0;
const STAT_ROW_HEIGHT: f32 = 28.0;

/// The chart's stats as left-aligned label/value cells in a two-pair grid:
/// `(text, x, y)` in the panel's y-up coordinates.
fn stat_grid(stepfile: &Stepfile, chart_index: usize) -> Vec<(String, f32, f32)> {
    let chart = &stepfile.charts[chart_index];
    let stats = chart.stats();
    let duration = chart
        .last_note_beat()
        .map(|beat| stepfile.timing.seconds_at_beat(beat))
        .unwrap_or(Seconds::ZERO);
    let minutes = (duration.0.max(0.0) / 60.0) as u32;
    let seconds = (duration.0.max(0.0) % 60.0) as u32;
    let pairs = [
        ("Steps", stats.steps.to_string()),
        ("Jumps", stats.jumps.to_string()),
        ("Holds", stats.holds.to_string()),
        ("Mines", stats.mines.to_string()),
        ("Length", format!("{minutes}:{seconds:02}")),
    ];
    let mut cells = Vec::new();
    for (index, (name, value)) in pairs.into_iter().enumerate() {
        let (label_x, value_x) = STAT_COLUMNS[index % STAT_COLUMNS.len()];
        let y = STAT_TOP_Y - (index / STAT_COLUMNS.len()) as f32 * STAT_ROW_HEIGHT;
        cells.push((name.to_string(), label_x, y));
        cells.push((value, value_x, y));
    }
    cells
}

fn difficulty_style(difficulty: &Difficulty) -> (&str, Color) {
    match difficulty {
        Difficulty::Beginner => ("Beginner", Color::from_rgb(0.35, 0.9, 0.95)),
        Difficulty::Easy => ("Basic", Color::from_rgb(0.95, 0.8, 0.25)),
        Difficulty::Medium => ("Difficult", Color::from_rgb(0.95, 0.35, 0.3)),
        Difficulty::Hard => ("Expert", Color::from_rgb(0.4, 0.95, 0.4)),
        Difficulty::Challenge => ("Challenge", Color::from_rgb(0.8, 0.45, 0.95)),
        Difficulty::Edit => ("Edit", Color::from_rgb(0.7, 0.7, 0.75)),
        Difficulty::Other(name) => (name.as_str(), Color::from_rgb(0.7, 0.7, 0.75)),
    }
}

fn bpm_label(stepfile: &Stepfile) -> String {
    match stepfile.display_bpm {
        Some(DisplayBpm::Single(bpm)) => format!("BPM {bpm}"),
        Some(DisplayBpm::Range(low, high)) => format!("BPM {low}-{high}"),
        Some(DisplayBpm::Random) => "BPM ???".to_string(),
        None => {
            let (low, high) = stepfile.timing.bpm_range();
            if (high.0 - low.0).abs() < 0.5 {
                format!("BPM {low}")
            } else {
                format!("BPM {low}-{high}")
            }
        }
    }
}
