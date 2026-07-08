use super::{
    BANNER_SIZE, BANNER_TEXT, BANNER_TINT, BPM_TEXT, DETAILS_BOX_CENTER, DETAILS_BOX_SIZE,
    PreferredDifficulty, STATS_TEXT, Wheel, WheelEntry, chart_for_preference,
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
use bevy::sprite::{SpriteImageMode, SpriteScalingMode};

#[derive(Component, Default, Clone)]
pub(super) struct InfoPanel;

/// Rebuilt from scratch: despawn-and-respawn beats mutating a panel of
/// text entities in place.
pub(super) fn refresh_info_panel(
    mut wheel: ResMut<Wheel>,
    library: Res<StepfileLibrary>,
    preferred: Res<PreferredDifficulty>,
    asset_server: Res<AssetServer>,
    panels: Query<Entity, With<InfoPanel>>,
    mut commands: Commands,
) {
    if !wheel.dirty {
        return;
    }
    wheel.dirty = false;
    for panel in &panels {
        commands.entity(panel).despawn();
    }
    let Some(entry) = wheel.entries.get(wheel.active).copied() else {
        return;
    };

    let (banner_path, fallback_title, headline, chart) = match entry {
        WheelEntry::Stepfile { id } => {
            let entry = library.stepfile(id);
            (
                entry.banner_path(),
                entry.display_title(),
                bpm_label(&entry.stepfile),
                chart_for_preference(&entry.stepfile, preferred.0).map(|index| (id, index)),
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
                None,
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
    let chart_lines: Vec<_> = chart
        .map(|(id, index)| {
            let stepfile = &library.stepfile(id).stepfile;
            let chart = &stepfile.charts[index];
            let (name, color) = difficulty_style(&chart.difficulty);
            vec![
                (format!("{name} {}", chart.meter), 34.0, color, 18.0),
                (stats_label(stepfile, index), 22.0, STATS_TEXT, -70.0),
            ]
        })
        .unwrap_or_default()
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

    commands.spawn_scoped(
        GameScene::FileSelect,
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
            ]
        },
    );
}

fn stats_label(stepfile: &Stepfile, chart_index: usize) -> String {
    let chart = &stepfile.charts[chart_index];
    let stats = chart.stats();
    let duration = chart
        .last_note_beat()
        .map(|beat| stepfile.timing.seconds_at_beat(beat))
        .unwrap_or(Seconds::ZERO);
    let minutes = (duration.0.max(0.0) / 60.0) as u32;
    let seconds = (duration.0.max(0.0) % 60.0) as u32;
    format!(
        "Steps {}   Jumps {}\nHolds {}   Mines {}\nLength {minutes}:{seconds:02}",
        stats.steps, stats.jumps, stats.holds, stats.mines
    )
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
        Some(DisplayBpm::Single(bpm)) => format!("BPM {bpm:.0}"),
        Some(DisplayBpm::Range(low, high)) => format!("BPM {low:.0}-{high:.0}"),
        Some(DisplayBpm::Random) => "BPM ???".to_string(),
        None => {
            let (low, high) = stepfile.timing.bpm_range();
            if (high - low).abs() < 0.5 {
                format!("BPM {low:.0}")
            } else {
                format!("BPM {low:.0}-{high:.0}")
            }
        }
    }
}
