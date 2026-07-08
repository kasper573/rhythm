use crate::core::persist::{load_user_data, save_user_data};
use crate::core::stepfile::Difficulty;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Best total points per played chart, keyed by [`highscore_key`]. Any
/// mutation is automatically persisted to `user_highscores.json` next to
/// the user settings; the file contains only the bare key→points map.
#[derive(Resource, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HighScores(BTreeMap<String, u32>);

impl HighScores {
    pub fn get(&self, key: &str) -> Option<u32> {
        self.0.get(key).copied()
    }

    /// Stores `points` if it beats the chart's best; returns whether it did.
    pub fn record(&mut self, key: String, points: u32) -> bool {
        if points > self.0.get(&key).copied().unwrap_or(0) {
            self.0.insert(key, points);
            true
        } else {
            false
        }
    }
}

/// One stable key per (group, stepfile, difficulty): the parts are joined
/// unambiguously and hashed, so the stored key is opaque and immune to
/// awkward characters in names. The difficulty's `Debug` form is the
/// canonical encoding — renaming a [`Difficulty`] variant orphans the
/// scores stored under it.
pub fn highscore_key(group_name: &str, stepfile_name: &str, difficulty: &Difficulty) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{group_name}\x1f{stepfile_name}\x1f{difficulty:?}"));
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

const HIGH_SCORES_FILE: &str = "user_highscores.json";

fn load_high_scores() -> HighScores {
    load_user_data(HIGH_SCORES_FILE)
}

/// Persists on every edit; the initial insertion is not an edit.
fn save_high_scores(high_scores: Res<HighScores>) {
    if high_scores.is_changed() && !high_scores.is_added() {
        save_user_data(HIGH_SCORES_FILE, &*high_scores);
    }
}
