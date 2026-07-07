use crate::core::units::Seconds;
use std::io::Cursor;
use std::path::Path;

/// Renders a complete audio track containing the tick sound effect at each
/// given time, producing WAV bytes ready to play alongside the music. Times
/// closer than one millisecond are deduplicated; times before the start of
/// the audio are skipped. `volume` scales the ticks and is clipped into the
/// output's sample range, so boosted configs can never exceed full scale.
pub fn render_tick_track(
    tick_wav: &Path,
    times: &[Seconds],
    volume: f32,
) -> Result<Vec<u8>, hound::Error> {
    let mut reader = hound::WavReader::open(tick_wav)?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let tick: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let scale = (1_i64 << (spec.bits_per_sample - 1)) as f32;
            downmix(
                &reader
                    .samples::<i32>()
                    .map(|sample| sample.map(|sample| sample as f32 / scale))
                    .collect::<Result<Vec<f32>, _>>()?,
                channels,
            )
        }
        hound::SampleFormat::Float => downmix(
            &reader.samples::<f32>().collect::<Result<Vec<f32>, _>>()?,
            channels,
        ),
    };

    let sample_rate = spec.sample_rate;
    let mut sorted: Vec<f64> = times
        .iter()
        .map(|time| time.0)
        .filter(|time| *time >= 0.0)
        .collect();
    sorted.sort_by(f64::total_cmp);
    sorted.dedup_by(|a, b| (*a - *b).abs() < 0.001);

    let last = sorted.last().copied().unwrap_or(0.0);
    let total_samples = ((last + 1.0) * sample_rate as f64) as usize + tick.len();
    let mut mix = vec![0.0f32; total_samples];
    for time in sorted {
        let start = (time * sample_rate as f64) as usize;
        for (offset, sample) in tick.iter().enumerate() {
            mix[start + offset] += sample;
        }
    }

    let mut bytes = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(
        &mut bytes,
        hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;
    for sample in mix {
        writer.write_sample(((sample * volume).clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(bytes.into_inner())
}

fn downmix(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}
