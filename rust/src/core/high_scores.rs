use crate::core::library::{StepfileId, StepfileLibrary};
use crate::core::persist::{load_user_data, save_user_data};
use crate::core::player::{PerPlayer, PlayerId};
use crate::core::stepfile::Chart;
use godot::classes::Engine;
use godot::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Each player's best total points per played chart, keyed by
/// [`highscore_key`] — a singleton. Recording an improvement persists it
/// immediately, one file per player; each file contains only the bare
/// key→points map.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct HighScores {
    books: PerPlayer<ScoreBook>,
    base: Base<Node>,
}

#[godot_api]
impl HighScores {
    pub fn singleton() -> Gd<HighScores> {
        Engine::singleton()
            .get_singleton("HighScores")
            .expect("HighScores singleton is registered at boot")
            .cast()
    }

    pub fn get(&self, player: PlayerId, key: &str) -> Option<u32> {
        self.books[player].0.get(key).copied()
    }

    /// Stores `points` if it beats the player's best on the chart; returns
    /// whether it did.
    pub fn record(&mut self, player: PlayerId, key: String, points: u32) -> bool {
        let book = &mut self.books[player];
        if points > book.0.get(&key).copied().unwrap_or(0) {
            book.0.insert(key, points);
            save_user_data(high_scores_file(player), book);
            true
        } else {
            false
        }
    }
}

#[godot_api]
impl INode for HighScores {
    /// Construction stays empty — the editor instantiates registered
    /// classes while scanning; the books load on entering the booted
    /// game's tree.
    fn init(base: Base<Node>) -> HighScores {
        HighScores {
            books: PerPlayer::default(),
            base,
        }
    }

    fn enter_tree(&mut self) {
        self.books = PerPlayer {
            p1: load_user_data(high_scores_file(PlayerId::P1)),
            p2: load_user_data(high_scores_file(PlayerId::P2)),
        };
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
    use sha2::{Digest, Sha256};
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

fn high_scores_file(player: PlayerId) -> &'static str {
    match player {
        PlayerId::P1 => "p1_highscores.json",
        PlayerId::P2 => "p2_highscores.json",
    }
}
