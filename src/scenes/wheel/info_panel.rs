use super::{
    BANNER_SIZE, BANNER_TEXT, BANNER_TINT, BPM_TEXT, DETAILS_BOX_CENTER, DETAILS_BOX_SIZE,
    PreferredDifficulty, STATS_TEXT, Wheel, WheelEntry,
};
use crate::core::assets::asset_server_path;
use crate::core::at;
use crate::core::font::game_font;
use crate::core::library::StepfileLibrary;
use crate::core::scene_flow::SpawnScoped;
use crate::core::stepfile::{Difficulty, DisplayBpm, Stepfile};
use crate::core::units::Seconds;
use crate::scenes::GameScene;
use bevy::prelude::*;
use bevy::sprite::{Anchor, SpriteImageMode, SpriteScalingMode};

#[derive(Component, Default, Clone)]
pub(super) struct InfoPanel;

/// Rebuilt from scratch: despawn-and-respawn beats mutating a panel of
/// text entities in place.
pub(super) fn refresh_info_panel(
    wheel: Res<Wheel>,
    library: Res<StepfileLibrary>,
    preferred: Res<PreferredDifficulty>,
    asset_server: Res<AssetServer>,
    panels: Query<Entity, With<InfoPanel>>,
    mut commands: Commands,
) {
    if !wheel.just_settled {
        return;
    }
    for panel in &panels {
        commands.entity(panel).despawn();
    }
    let Some(entry) = wheel.entries.get(wheel.active).copied() else {
        return;
    };

    let (banner_path, fallback_title, headline, charts) = match entry {
        WheelEntry::Stepfile { id } => {
            let entry = library.stepfile(id);
            let charts: Vec<_> = wheel
                .players
                .iter()
                .filter_map(|player| {
                    wheel
                        .chart_for(&entry.stepfile, &preferred, *player)
                        .map(|index| (*player, id, index))
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
            let group = &library.groups[index];
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
    let banner_path = banner_path.or_else(|| library.default_bgm.banner_path());

    // Real banners cover the fixed rect like a CSS `object-fit: cover`
    // image — scaled to fill, centered, overflow cropped, never stretched.
    // The generated placeholder is made to stretch.
    let (image, mode, tint, title) = match banner_path.as_deref().and_then(asset_server_path) {
        Some(path) => (
            asset_server.load(path),
            SpriteImageMode::Scale(SpriteScalingMode::FillCenter),
            Color::WHITE,
            None,
        ),
        None => (
            wheel.bar_image.clone(),
            SpriteImageMode::Auto,
            BANNER_TINT,
            Some(fallback_title),
        ),
    };
    // The banner sits flush against the details box's top and sides.
    let banner_y = DETAILS_BOX_CENTER.y + (DETAILS_BOX_SIZE.y - BANNER_SIZE.y) / 2.0;
    let title: Vec<_> = title
        .map(|title| {
            bsn! {
                game_font(24.0)
                Text2d({title})
                TextColor({BANNER_TEXT})
                at(0.0, banner_y, 0.5)
            }
        })
        .into_iter()
        .collect();
    // One difficulty line per active player (tagged when there are two),
    // with the chart stats below in single-player modes; versus swaps the
    // stats for the second player's line.
    let mut lines: Vec<(String, f32, Color, f32)> = Vec::new();
    let mut stat_cells: Vec<(String, f32, f32)> = Vec::new();
    let tagged = charts.len() > 1;
    for (row, (player, id, index)) in charts.iter().enumerate() {
        let stepfile = &library.stepfile(*id).stepfile;
        let chart = &stepfile.charts[*index];
        let (name, color) = difficulty_style(&chart.difficulty);
        let line = if tagged {
            format!("{}  {name} {}", player.label(), chart.meter)
        } else {
            format!("{name} {}", chart.meter)
        };
        lines.push((line, 34.0, color, 18.0 - row as f32 * 42.0));
        if !tagged {
            stat_cells = stat_grid(stepfile, *index);
        }
    }
    let chart_lines: Vec<_> = lines
        .into_iter()
        .map(|(line, size, color, y)| {
            bsn! {
                game_font(size)
                Text2d({line})
                TextColor({color})
                at(0.0, y, 0.0)
            }
        })
        .collect();
    let stat_cells: Vec<_> = stat_cells
        .into_iter()
        .map(|(text, x, y)| {
            bsn! {
                game_font(22.0)
                Text2d({text})
                TextColor({STATS_TEXT})
                Anchor({Anchor::CENTER_LEFT.0})
                at(x, y, 0.0)
            }
        })
        .collect();

    commands.spawn_scoped(
        GameScene::Wheel,
        bsn! {
            InfoPanel
            at(-320.0, 0.0, 5.0)
            Visibility::default()
            Children [
                (
                    Sprite {
                        image: {image},
                        color: {tint},
                        custom_size: {Some(BANNER_SIZE)},
                        image_mode: {mode},
                    }
                    at(0.0, banner_y, 0.0)
                ),
                {title},
                (
                    game_font(28.0)
                    Text2d({headline})
                    TextColor({BPM_TEXT})
                    at(0.0, 70.0, 0.0)
                ),
                {chart_lines},
                {stat_cells},
            ]
        },
    );
}

/// Horizontal starts of the stat grid's label and value columns, two
/// pairs side by side, in panel-local coordinates.
const STAT_COLUMNS: [(f32, f32); 2] = [(-170.0, -75.0), (35.0, 130.0)];
const STAT_TOP_Y: f32 = -48.0;
const STAT_ROW_HEIGHT: f32 = 28.0;

/// The chart's stats as left-aligned label/value cells in a two-pair grid:
/// `(text, x, y)` for [`refresh_info_panel`] to place.
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
    for (index, (label, value)) in pairs.into_iter().enumerate() {
        let (label_x, value_x) = STAT_COLUMNS[index % STAT_COLUMNS.len()];
        let y = STAT_TOP_Y - (index / STAT_COLUMNS.len()) as f32 * STAT_ROW_HEIGHT;
        cells.push((label.to_string(), label_x, y));
        cells.push((value, value_x, y));
    }
    cells
}

fn difficulty_style(difficulty: &Difficulty) -> (&str, Color) {
    match difficulty {
        Difficulty::Beginner => ("Beginner", Color::srgb(0.35, 0.9, 0.95)),
        Difficulty::Easy => ("Basic", Color::srgb(0.95, 0.8, 0.25)),
        Difficulty::Medium => ("Difficult", Color::srgb(0.95, 0.35, 0.3)),
        Difficulty::Hard => ("Expert", Color::srgb(0.4, 0.95, 0.4)),
        Difficulty::Challenge => ("Challenge", Color::srgb(0.8, 0.45, 0.95)),
        Difficulty::Edit => ("Edit", Color::srgb(0.7, 0.7, 0.75)),
        Difficulty::Other(name) => (name.as_str(), Color::srgb(0.7, 0.7, 0.75)),
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
