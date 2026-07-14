using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The play stage's backgrounds: the stepfile's background timeline of
/// media covers, cued on the musical timeline, cross-faded, and paced by
/// the session's visible clock so videos stay locked to the music. Layers
/// live in their own host under everything else the scene draws.
/// </summary>
internal sealed class Backgrounds : IDisposable
{
    private const float CrossfadeSeconds = 0.5f;
    private const float Dim = 0.5f;

    private Control host;
    private string? initialBgPath;
    private List<BackgroundChange> changes = [];
    private int nextChangeIndex;
    private List<Layer> layers = [];

    private sealed class BackgroundChange
    {
        public Seconds Time { get; set; }
        public string Path { get; set; } = "";
        public bool Crossfade { get; set; }
        public bool Loops { get; set; }
    }

    private sealed class Layer
    {
        public MediaCover? Cover { get; set; }
        public float Alpha { get; set; }
        public float Target { get; set; }
    }

    public Backgrounds(Control scene, StepfileEntry entry, StepfileTiming timing)
    {
        host = new Control();
        host.SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
        host.MouseFilter = Control.MouseFilterEnum.Ignore;
        scene.AddChild(host);

        // Build background change timeline
        foreach (var change in entry.Stepfile.BgChanges)
        {
            var resolvedPath = entry.ResolveFile(change.File);
            if (resolvedPath == null)
            {
                continue;
            }

            changes.Add(new BackgroundChange
            {
                Time = timing.SecondsAtBeat(change.Beat),
                Path = resolvedPath,
                Crossfade = change.Crossfade,
                Loops = change.Loops,
            });
        }

        changes.Sort((a, b) => a.Time.Value.CompareTo(b.Time.Value));
        initialBgPath = entry.BackgroundPath();
    }

    /// <summary>
    /// Cues due background changes, runs the cross-fades, and locks every
    /// video to the session's visible timeline.
    /// </summary>
    public void Update(Seconds visible, float delta)
    {
        // Apply initial background if not yet done
        if (initialBgPath != null)
        {
            Apply(Seconds.Zero, initialBgPath, false, false);
            initialBgPath = null;
        }

        // Trigger due changes
        while (nextChangeIndex < changes.Count && changes[nextChangeIndex].Time.Value <= visible.Value)
        {
            var change = changes[nextChangeIndex];
            Apply(change.Time, change.Path, change.Crossfade, change.Loops);
            nextChangeIndex++;
        }

        // Advance fades
        var step = delta / CrossfadeSeconds;
        layers = layers.Where(layer =>
        {
            if (layer.Cover == null)
            {
                return false;
            }

            var next = layer.Target > layer.Alpha
                ? Math.Min(layer.Alpha + step, layer.Target)
                : Math.Max(layer.Alpha - step, layer.Target);

            if (Math.Abs(next - layer.Alpha) > 0.0001f)
            {
                layer.Alpha = next;
                var color = layer.Cover.Modulate;
                color.A = Screen.LinearBlend(next);
                layer.Cover.Modulate = color;
            }

            if (layer.Target <= 0.0f && next <= 0.0f)
            {
                layer.Cover?.QueueFree();
                return false;
            }

            layer.Cover.SetClock(visible);
            return true;
        }).ToList();
    }

    private void Apply(Seconds time, string path, bool crossfade, bool loops)
    {
        var alpha = crossfade ? 0.0f : 1.0f;
        var color = new Color(Dim, Dim, Dim, Screen.LinearBlend(alpha));

        var cover = MediaCover.Create(path, color, 0, time, loops, MediaPace.Manual);
        if (cover == null)
        {
            // Unshowable cue keeps current background
            return;
        }

        // Set fade targets for existing layers
        foreach (var layer in layers)
        {
            if (crossfade)
            {
                layer.Target = 0.0f;
            }
            else
            {
                layer.Cover?.QueueFree();
            }
        }

        if (!crossfade)
        {
            layers.Clear();
        }

        host.AddChild(cover);
        layers.Add(new Layer
        {
            Cover = cover,
            Alpha = alpha,
            Target = 1.0f,
        });
    }

    /// <summary>Frees the background host and its layers when the stage tears down.</summary>
    public void Dispose()
    {
        if (GodotObject.IsInstanceValid(host))
        {
            host.QueueFree();
        }
    }
}
