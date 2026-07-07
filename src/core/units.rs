use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};

/// A duration or point in time measured in seconds, typically on the audio clock.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
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

impl fmt::Display for Seconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}s", self.0)
    }
}

impl Add for Seconds {
    type Output = Seconds;
    fn add(self, rhs: Seconds) -> Seconds {
        Seconds(self.0 + rhs.0)
    }
}

impl AddAssign for Seconds {
    fn add_assign(&mut self, rhs: Seconds) {
        self.0 += rhs.0;
    }
}

impl Sub for Seconds {
    type Output = Seconds;
    fn sub(self, rhs: Seconds) -> Seconds {
        Seconds(self.0 - rhs.0)
    }
}

impl Mul<f64> for Seconds {
    type Output = Seconds;
    fn mul(self, rhs: f64) -> Seconds {
        Seconds(self.0 * rhs)
    }
}

impl Div<Seconds> for Seconds {
    type Output = f64;
    fn div(self, rhs: Seconds) -> f64 {
        self.0 / rhs.0
    }
}

impl Neg for Seconds {
    type Output = Seconds;
    fn neg(self) -> Seconds {
        Seconds(-self.0)
    }
}

/// A whole millisecond count, used where settings are expressed in milliseconds.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize, Hash,
)]
pub struct Millis(pub i64);

impl Millis {
    pub fn to_seconds(self) -> Seconds {
        Seconds(self.0 as f64 / 1000.0)
    }
}

impl fmt::Display for Millis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ms", self.0)
    }
}

impl Add for Millis {
    type Output = Millis;
    fn add(self, rhs: Millis) -> Millis {
        Millis(self.0 + rhs.0)
    }
}

/// A musical position measured in beats from beat zero of a stepfile.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
pub struct Beat(pub f64);

impl Beat {
    pub const ZERO: Beat = Beat(0.0);

    /// Fractional position within the current beat, in `0.0..1.0`.
    pub fn phase(self) -> f64 {
        self.0.rem_euclid(1.0)
    }
}

impl fmt::Display for Beat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "beat {:.3}", self.0)
    }
}

impl Add for Beat {
    type Output = Beat;
    fn add(self, rhs: Beat) -> Beat {
        Beat(self.0 + rhs.0)
    }
}

impl Sub for Beat {
    type Output = Beat;
    fn sub(self, rhs: Beat) -> Beat {
        Beat(self.0 - rhs.0)
    }
}
