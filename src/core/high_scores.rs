use crate::core::stepfile::Difficulty;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;

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
/// awkward characters in names.
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

fn high_scores_path() -> PathBuf {
    dirs::config_dir()
        .expect("no OS config directory available to store high scores")
        .join("rhythm")
        .join("user_highscores.json")
}

fn load_high_scores() -> HighScores {
    let path = high_scores_path();
    if !path.exists() {
        return HighScores::default();
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    crate::core::jsonc::parse(&text)
        .unwrap_or_else(|error| panic!("invalid high scores file {}: {error}", path.display()))
}

/// Persists on every edit; the initial insertion is not an edit.
fn save_high_scores(high_scores: Res<HighScores>) {
    if !high_scores.is_changed() || high_scores.is_added() {
        return;
    }
    let path = high_scores_path();
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(path.parent().expect("high scores path has a parent"))?;
        let json =
            serde_json::to_string_pretty(&*high_scores).expect("high scores always serialize");
        std::fs::write(&path, json)
    };
    match write() {
        Ok(()) => info!("high scores saved to {}", path.display()),
        Err(error) => error!("failed to save high scores to {}: {error}", path.display()),
    }
}
