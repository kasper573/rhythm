use crate::core::stepfile::StepsType;
use bevy::prelude::*;
use std::ops::{Index, IndexMut};
use strum::{EnumIter, IntoStaticStr};

/// One of the two player slots the machine offers. Every player-scoped
/// piece of state — settings, high scores, key bindings, a play session's
/// stage — is keyed by this. `Default` exists only because BSN's
/// `FromTemplate` derive demands it of component fields; no game logic
/// falls back to it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, EnumIter, IntoStaticStr)]
pub enum PlayerId {
    #[default]
    P1,
    P2,
}

impl PlayerId {
    pub fn label(self) -> &'static str {
        self.into()
    }
}

/// One value per player slot, for state that always exists for both
/// players regardless of how many are active.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PerPlayer<T> {
    pub p1: T,
    pub p2: T,
}

impl<T> Index<PlayerId> for PerPlayer<T> {
    type Output = T;

    fn index(&self, player: PlayerId) -> &T {
        match player {
            PlayerId::P1 => &self.p1,
            PlayerId::P2 => &self.p2,
        }
    }
}

impl<T> IndexMut<PlayerId> for PerPlayer<T> {
    fn index_mut(&mut self, player: PlayerId) -> &mut T {
        match player {
            PlayerId::P1 => &mut self.p1,
            PlayerId::P2 => &mut self.p2,
        }
    }
}

/// How the machine is being played: who is on it and which charts they
/// step. Selected on the mode select scene and read by every scene after
/// it.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, EnumIter, IntoStaticStr)]
pub enum PlayMode {
    #[default]
    Singles,
    Doubles,
    Versus,
}

impl PlayMode {
    pub fn label(self) -> &'static str {
        self.into()
    }

    /// The chart type this mode plays.
    pub fn steps_type(self) -> StepsType {
        match self {
            PlayMode::Singles | PlayMode::Versus => StepsType::DanceSingle,
            PlayMode::Doubles => StepsType::DanceDouble,
        }
    }

    /// The active player slots: P1 alone plays singles and doubles (the
    /// doubles chart spans both pads), versus fields both.
    pub fn players(self) -> &'static [PlayerId] {
        match self {
            PlayMode::Singles | PlayMode::Doubles => &[PlayerId::P1],
            PlayMode::Versus => &[PlayerId::P1, PlayerId::P2],
        }
    }
}
