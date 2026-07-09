use crate::core::library::{StepfileId, StepfileLibrary};
use crate::core::persist::{load_user_data, save_user_data};
use crate::core::player::{PerPlayer, PlayerId};
use crate::core::stepfile::Chart;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Each player's best total points per played chart, keyed by
/// [`highscore_key`]. Any mutation is automatically persisted, one file
/// per player next to the settings; each file contains only the bare
/// key→points map.
#[derive(Resource, Default)]
pub struct HighScores(PerPlayer<ScoreBook>);

impl HighScores {
    pub fn get(&self, player: PlayerId, key: &str) -> Option<u32> {
        self.0[player].0.get(key).copied()
    }

    /// Stores `points` if it beats the player's best on the chart; returns
    /// whether it did.
    pub fn record(&mut self, player: PlayerId, key: String, points: u32) -> bool {
        let book = &mut self.0[player].0;
        if points > book.get(&key).copied().unwrap_or(0) {
            book.insert(key, points);
            true
        } else {
            false
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(transparent)]
struct ScoreBook(BTreeMap<String, u32>);

/// One stable key per (group, stepfile, chart type, difficulty): the parts
/// are joined unambiguously and hashed, so the stored key is opaque and
/// immune to awkward characters in names. The chart type's and difficulty's
/// `Debug` forms are the canonical encoding — renaming a
/// [`StepsType`](crate::core::stepfile::StepsType) or
/// [`Difficulty`](crate::core::stepfile::Difficulty) variant orphans the
/// scores stored under it.
pub fn highscore_key(library: &StepfileLibrary, id: StepfileId, chart: &Chart) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!(
        "{}\x1f{}\x1f{:?}\x1f{:?}",
        library.group_name(id),
        library.stepfile(id).name(),
        chart.steps_type,
        chart.difficulty,
    ));
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub struct HighScoresPlugin;

impl Plugin for HighScoresPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load_high_scores())
            .add_systems(Update, save_high_scores);
    }
}

fn high_scores_file(player: PlayerId) -> &'static str {
    match player {
        PlayerId::P1 => "p1_highscores.json",
        PlayerId::P2 => "p2_highscores.json",
    }
}

fn load_high_scores() -> HighScores {
    HighScores(PerPlayer {
        p1: load_user_data(high_scores_file(PlayerId::P1)),
        p2: load_user_data(high_scores_file(PlayerId::P2)),
    })
}

/// Persists on every edit; the initial insertion is not an edit.
fn save_high_scores(high_scores: Res<HighScores>) {
    if high_scores.is_changed() && !high_scores.is_added() {
        save_user_data(high_scores_file(PlayerId::P1), &high_scores.0.p1);
        save_user_data(high_scores_file(PlayerId::P2), &high_scores.0.p2);
    }
}
