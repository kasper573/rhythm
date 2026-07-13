using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// Wheel partial: the scene background wash — the active row's background
/// (image or looping video) over the green backdrop, as stacked media covers.
/// Changing rows cross-fades: the incoming layer waits invisible until its
/// image has loaded, then retires every older layer while it eases in — so the
/// old background always fades under a renderable new one, never against a gap.
/// </summary>
public partial class Wheel
{
    private void RefreshWash()
    {
        if (!justSettled)
        {
            return;
        }

        // Rows without a background of their own fall back to the default
        // BGM's, so the scene always has one to show.
        var path = (entries.Count > active && entries[active] is WheelEntry.Stepfile row
            ? Library.Instance.Stepfile(row.Id).BackgroundPath()
            : null) ?? Library.Instance.DefaultBgm.BackgroundPath();

        if (path is null)
        {
            // Nothing to show at all: fade everything out.
            foreach (var layer in wash.Layers)
            {
                layer.Target = 0.0f;
            }
            return;
        }

        if (wash.Layers.Exists(layer => layer.Target > 0.0f && layer.Source == path))
        {
            return;
        }

        var cover = MediaCover.Create(path, new Color(1, 1, 1, 0), 5, Seconds.Zero, looping: true, MediaPace.Wall);
        if (cover is null)
        {
            return;
        }

        // Later siblings draw above the layers fading out beneath them.
        AddChild(cover);
        wash.Sequence++;
        wash.Layers.Add(new WashLayer
        {
            Cover = cover,
            Target = Screen.LinearBlend(BackgroundOpacity),
            Sequence = wash.Sequence,
            Source = path,
        });
    }

    /// <summary>
    /// Eases every wash layer toward its target opacity at the wheel's settle
    /// rate and retires the fully faded-out ones. Layers whose image is still
    /// loading hold at zero: only a loaded layer may lead, and only the leader
    /// retires the layers beneath it.
    /// </summary>
    private void FadeWash(double delta)
    {
        uint? leader = null;
        foreach (var layer in wash.Layers)
        {
            if (layer.Target > 0.0f && layer.Cover.IsReady && (leader is null || layer.Sequence > leader))
            {
                leader = layer.Sequence;
            }
        }
        if (leader is uint leaderSequence)
        {
            foreach (var layer in wash.Layers)
            {
                if (layer.Sequence < leaderSequence && layer.Target > 0.0f)
                {
                    layer.Target = 0.0f;
                }
            }
        }

        var ease = 1.0f - Mathf.Exp(-WheelEaseRate * (float)delta);
        for (int i = wash.Layers.Count - 1; i >= 0; i--)
        {
            var layer = wash.Layers[i];
            if (layer.Target > 0.0f && !layer.Cover.IsReady)
            {
                continue;
            }
            var modulate = layer.Cover.Modulate;
            var next = modulate.A + (layer.Target - modulate.A) * ease;
            if (Mathf.Abs(next - layer.Target) < 0.002f)
            {
                next = layer.Target;
            }
            if (next != modulate.A)
            {
                modulate.A = next;
                layer.Cover.Modulate = modulate;
            }
            if (layer.Target <= 0.0f && next <= 0.0f)
            {
                layer.Cover.QueueFree();
                wash.Layers.RemoveAt(i);
            }
        }
    }

    private sealed class WashLayer
    {
        public required MediaCover Cover { get; init; }
        public required float Target { get; set; }
        public required uint Sequence { get; init; }
        public required string Source { get; init; }
    }

    private sealed class Wash
    {
        public List<WashLayer> Layers { get; } = [];
        public uint Sequence { get; set; }
    }
}
