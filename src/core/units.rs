use derive_more::{Add, AddAssign, Display, Mul, Neg, Sub};
use serde::{Deserialize, Serialize};
use std::ops::Div;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    PartialOrd,
    Default,
    Serialize,
    Deserialize,
    Add,
    AddAssign,
    Sub,
    Mul,
    Neg,
    Display,
)]
#[display("{_0:.3}s")]
pub struct Seconds(pub f64);

impl Seconds {
    pub const ZERO: Seconds = Seconds(0.0);

    pub fn from_millis(millis: f64) -> Self {
        Seconds(millis / 1000.0)
    }

    pub fn to_millis(self) -> f64 {
        self.0 * 1000.0
    }

    pub fn abs(self) -> Self {
        Seconds(self.0.abs())
    }

    pub fn max(self, other: Self) -> Self {
        Seconds(self.0.max(other.0))
    }

    pub fn min(self, other: Self) -> Self {
        Seconds(self.0.min(other.0))
    }
}

impl Div<Seconds> for Seconds {
    type Output = f64;
    fn div(self, rhs: Seconds) -> f64 {
        self.0 / rhs.0
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    Serialize,
    Deserialize,
    Hash,
    Add,
    Display,
)]
#[display("{_0}ms")]
pub struct Millis(pub i64);

impl Millis {
    pub fn to_seconds(self) -> Seconds {
        Seconds(self.0 as f64 / 1000.0)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize, Add, Sub, Display,
)]
#[display("beat {_0:.3}")]
pub struct Beat(pub f64);

impl Beat {
    pub const ZERO: Beat = Beat(0.0);

    pub fn phase(self) -> f64 {
        self.0.rem_euclid(1.0)
    }
}
