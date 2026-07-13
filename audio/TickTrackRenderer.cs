using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// Builds the play stage's tick track: the tick sample mixed in at every row
/// time, as one PCM stream to play alongside the music. Times closer than a
/// millisecond collapse into one; times before zero are dropped; the volume
/// scales the ticks and clips at full scale so a boosted config never wraps.
/// </summary>
public static class TickTrackRenderer
{
    /// <summary>The tick track for the given row times, or null if the sample isn't linear PCM.</summary>
    public static AudioStreamWav? Render(byte[] tickWav, IEnumerable<Seconds> times, float volume)
    {
        if (AudioStreamWav.LoadFromBuffer(tickWav) is not { } tick || MonoSamples(tick) is not { } sample)
        {
            return null;
        }
        var rate = tick.MixRate;

        var sorted = times.Select(time => time.Value).Where(time => time >= 0.0).OrderBy(time => time).ToList();
        var mixTimes = new List<double>();
        foreach (var time in sorted)
        {
            if (mixTimes.Count == 0 || time - mixTimes[^1] >= 0.001)
            {
                mixTimes.Add(time);
            }
        }

        var last = mixTimes.Count > 0 ? mixTimes[^1] : 0.0;
        var mix = new float[(int)((last + 1.0) * rate) + sample.Length];
        foreach (var time in mixTimes)
        {
            var start = (int)(time * rate);
            for (int i = 0; i < sample.Length; i++)
            {
                mix[start + i] += sample[i];
            }
        }

        var data = new byte[mix.Length * 2];
        for (int i = 0; i < mix.Length; i++)
        {
            var value = (short)(Math.Clamp(mix[i] * volume, -1.0f, 1.0f) * short.MaxValue);
            data[i * 2] = (byte)(value & 0xFF);
            data[(i * 2) + 1] = (byte)((value >> 8) & 0xFF);
        }

        return new AudioStreamWav
        {
            Data = data,
            Format = AudioStreamWav.FormatEnum.Format16Bits,
            MixRate = rate,
            Stereo = false,
        };
    }

    /// <summary>The decoded stream's samples, downmixed to mono floats; null unless 8- or 16-bit PCM.</summary>
    private static float[]? MonoSamples(AudioStreamWav wav)
    {
        var data = wav.Data;
        var channels = wav.Stereo ? 2 : 1;
        switch (wav.Format)
        {
            case AudioStreamWav.FormatEnum.Format16Bits:
                var frames16 = data.Length / 2 / channels;
                var mono16 = new float[frames16];
                for (int frame = 0; frame < frames16; frame++)
                {
                    var sum = 0.0f;
                    for (int channel = 0; channel < channels; channel++)
                    {
                        var index = ((frame * channels) + channel) * 2;
                        sum += (short)(data[index] | (data[index + 1] << 8)) / 32768.0f;
                    }
                    mono16[frame] = sum / channels;
                }
                return mono16;

            case AudioStreamWav.FormatEnum.Format8Bits:
                var frames8 = data.Length / channels;
                var mono8 = new float[frames8];
                for (int frame = 0; frame < frames8; frame++)
                {
                    var sum = 0.0f;
                    for (int channel = 0; channel < channels; channel++)
                    {
                        sum += (sbyte)data[(frame * channels) + channel] / 128.0f;
                    }
                    mono8[frame] = sum / channels;
                }
                return mono8;

            default:
                return null;
        }
    }
}
