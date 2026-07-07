use crate::core::units::{Beat, Seconds};

/// All `Seconds` values are positions on the audio clock of the music file:
/// beat zero maps to `-offset` seconds, matching the .sm `#OFFSET` convention.
#[derive(Debug, Clone, PartialEq)]
pub struct StepfileTiming {
    anchors: Vec<Anchor>,
}

impl StepfileTiming {
    /// `bpms` are `(beat, bpm)` pairs and `stops` are `(beat, duration)`
    /// pairs, both as written in the .sm file. Entries with non-positive BPM
    /// are ignored.
    pub fn new(offset: Seconds, bpms: &[(Beat, f64)], stops: &[(Beat, Seconds)]) -> StepfileTiming {
        StepfileTiming {
            anchors: build_anchors(offset, bpms, stops),
        }
    }

    pub fn seconds_at_beat(&self, beat: Beat) -> Seconds {
        let anchor = self.anchor_before_beat(beat);
        if anchor.beats_per_second <= 0.0 {
            return Seconds(anchor.seconds);
        }
        Seconds(anchor.seconds + (beat.0 - anchor.beat) / anchor.beats_per_second)
    }

    pub fn beat_at_seconds(&self, seconds: Seconds) -> Beat {
        let anchor = self.anchor_before_seconds(seconds);
        Beat(anchor.beat + (seconds.0 - anchor.seconds) * anchor.beats_per_second)
    }

    pub fn beat_phase(&self, seconds: Seconds) -> f64 {
        self.beat_at_seconds(seconds).phase()
    }

    pub fn bpm_range(&self) -> (f64, f64) {
        self.anchors
            .iter()
            .map(|a| a.beats_per_second * 60.0)
            .filter(|bpm| *bpm > 0.0)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), bpm| {
                (min.min(bpm), max.max(bpm))
            })
    }

    fn anchor_before_beat(&self, beat: Beat) -> Anchor {
        let index = self.anchors.partition_point(|a| a.beat < beat.0);
        self.anchors[index.saturating_sub(1)]
    }

    fn anchor_before_seconds(&self, seconds: Seconds) -> Anchor {
        let index = self.anchors.partition_point(|a| a.seconds <= seconds.0);
        self.anchors[index.saturating_sub(1)]
    }
}

/// A point where the beat↔seconds mapping changes slope. From this anchor
/// until the next one, beats advance at `beats_per_second` (zero during a
/// stop).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Anchor {
    beat: f64,
    seconds: f64,
    beats_per_second: f64,
}

fn build_anchors(offset: Seconds, bpms: &[(Beat, f64)], stops: &[(Beat, Seconds)]) -> Vec<Anchor> {
    enum Change {
        Bpm(f64),
        Stop(f64),
    }

    let mut changes: Vec<(f64, Change)> = bpms
        .iter()
        .filter(|(_, bpm)| *bpm > 0.0)
        .map(|(beat, bpm)| (beat.0.max(0.0), Change::Bpm(bpm / 60.0)))
        .chain(
            stops
                .iter()
                .filter(|(_, duration)| duration.0 > 0.0)
                .map(|(beat, duration)| (beat.0.max(0.0), Change::Stop(duration.0))),
        )
        .collect();
    // At equal beats a BPM change applies before a stop, so time frozen by
    // the stop resumes at the new tempo.
    changes.sort_by(|(a_beat, a), (b_beat, b)| {
        a_beat.total_cmp(b_beat).then_with(|| {
            let order = |c: &Change| match c {
                Change::Bpm(_) => 0,
                Change::Stop(_) => 1,
            };
            order(a).cmp(&order(b))
        })
    });

    let initial_bps = changes
        .iter()
        .find_map(|(_, change)| match change {
            Change::Bpm(bps) => Some(*bps),
            Change::Stop(_) => None,
        })
        .unwrap_or(120.0 / 60.0);

    let mut anchors = vec![Anchor {
        beat: 0.0,
        seconds: -offset.0,
        beats_per_second: initial_bps,
    }];
    let mut beat = 0.0;
    let mut seconds = -offset.0;
    let mut bps = initial_bps;

    for (change_beat, change) in changes {
        seconds += (change_beat - beat) / bps;
        beat = change_beat;
        match change {
            Change::Bpm(new_bps) => {
                bps = new_bps;
                anchors.push(Anchor {
                    beat,
                    seconds,
                    beats_per_second: bps,
                });
            }
            Change::Stop(duration) => {
                anchors.push(Anchor {
                    beat,
                    seconds,
                    beats_per_second: 0.0,
                });
                seconds += duration;
                anchors.push(Anchor {
                    beat,
                    seconds,
                    beats_per_second: bps,
                });
            }
        }
    }

    anchors
}
