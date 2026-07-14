using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The machine-tuning controls live during play: toggling the tick track,
/// AutoSync, and nudging the three synchronization offsets — all surfacing
/// through the offset OSD.
/// </summary>
internal sealed class Tuning : IDisposable
{
    private const int AutosyncSamples = 24;

    private bool autosyncEnabled;
    private List<Seconds> samples = [];
    private (bool autosyncEnabled, int sampleCount)? shown;
    private Label? osd;
    private Label? status;
    private float osdAlpha;

    public Tuning(Control scene)
    {
        osd = new Label
        {
            Text = "",
            Modulate = new Color(1.0f, 1.0f, 1.0f, 0.0f),
        };
        osd.SetAnchorsPreset(Control.LayoutPreset.BottomRight);
        osd.HorizontalAlignment = HorizontalAlignment.Right;
        osd.Position = new Vector2(-424.0f, -40.0f);
        osd.Size = new Vector2(400.0f, 30.0f);
        osd.AddThemeFontSizeOverride("font_sizes", 24);
        scene.AddChild(osd);

        status = new Label
        {
            Text = "",
            Modulate = new Color(0.5f, 0.9f, 1.0f, 1.0f),
            Visible = false,
        };
        status.SetAnchorsPreset(Control.LayoutPreset.BottomRight);
        status.HorizontalAlignment = HorizontalAlignment.Right;
        status.Position = new Vector2(-424.0f, -72.0f);
        status.Size = new Vector2(400.0f, 30.0f);
        status.AddThemeFontSizeOverride("font_sizes", 24);
        scene.AddChild(status);
    }

    /// <summary>
    /// Samples every banked press's timing error the engine reports.
    /// </summary>
    public void PushSample(Seconds error)
    {
        if (autosyncEnabled)
        {
            samples.Add(error);
        }
    }

    /// <summary>
    /// Updates tuning controls and displays: tick toggle, AutoSync, and
    /// offset adjustments.
    /// </summary>
    public void Update(SoundChannel? tick, double delta)
    {
        // Toggle tick audio
        if (Actions.JustPressed(GameAction.ToggleTickAudio) && tick != null)
        {
            tick.SetMuted(!tick.IsMuted);
        }

        // Toggle AutoSync
        if (Actions.JustPressed(GameAction.ToggleAutoSync))
        {
            autosyncEnabled = !autosyncEnabled;
            samples.Clear();
        }

        // Process AutoSync samples
        if (autosyncEnabled && samples.Count >= AutosyncSamples)
        {
            var sortedSamples = samples.OrderBy(s => s.Value).ToList();
            var median = sortedSamples[sortedSamples.Count / 2];
            var deltaMillis = new Millis((long)Math.Round(median.ToMillis()));

            if (deltaMillis.Value != 0)
            {
                var settings = Settings.Instance;
                settings.EditMachine(machine =>
                {
                    return machine with
                    {
                        Timing = machine.Timing with
                        {
                            MachineOffset = machine.Timing.MachineOffset + deltaMillis
                        }
                    };
                });

                var offset = Settings.Instance.Machine.Timing.MachineOffset;
                Flash($"Machine offset: {offset}");
            }

            samples.Clear();
        }

        // Update AutoSync status display
        var state = (autosyncEnabled, samples.Count);
        if (shown != state)
        {
            shown = state;
            if (status != null)
            {
                if (autosyncEnabled)
                {
                    status.Text = $"AutoSync ({samples.Count}/{AutosyncSamples} samples)";
                    status.Visible = true;
                }
                else
                {
                    status.Visible = false;
                }
            }
        }

        // Adjust timing offsets
        AdjustTimingOffsets();

        // Fade OSD
        if (osdAlpha > 0.0f)
        {
            osdAlpha = Math.Max(osdAlpha - (float)delta, 0.0f);
            if (osd != null)
            {
                var color = osd.Modulate;
                color.A = osdAlpha;
                osd.Modulate = color;
            }
        }
    }

    private void Flash(string line)
    {
        if (osd != null)
        {
            osd.Text = line;
            osdAlpha = 1.0f;
        }
    }

    private void AdjustTimingOffsets()
    {
        var step = IsShiftHeld() ? 10L : 1L;
        var pairs = new[]
        {
            (GameAction.DecreaseMachineOffset, GameAction.IncreaseMachineOffset),
            (GameAction.DecreaseVisualDelay, GameAction.IncreaseVisualDelay),
            (GameAction.DecreaseAudioLatency, GameAction.IncreaseAudioLatency),
        };

        string? osdLine = null;
        for (int i = 0; i < pairs.Length; i++)
        {
            var (decrease, increase) = pairs[i];
            long delta = 0;

            if (Actions.JustPressed(increase))
            {
                delta += step;
            }
            if (Actions.JustPressed(decrease))
            {
                delta -= step;
            }

            if (delta == 0)
            {
                continue;
            }

            var settings = Settings.Instance;
            settings.EditMachine(machine =>
            {
                var timing = machine.Timing;
                return i switch
                {
                    0 => machine with
                    {
                        Timing = timing with
                        {
                            MachineOffset = timing.MachineOffset + new Millis(delta)
                        }
                    },
                    1 => machine with
                    {
                        Timing = timing with
                        {
                            VisualDelay = timing.VisualDelay + new Millis(delta)
                        }
                    },
                    _ => machine with
                    {
                        Timing = timing with
                        {
                            AudioLatency = (timing.AudioLatency ?? new Millis(0)) + new Millis(delta)
                        }
                    },
                };
            });

            var updatedTiming = Settings.Instance.Machine.Timing;
            osdLine = i switch
            {
                0 => $"Machine offset: {updatedTiming.MachineOffset}",
                1 => $"Visual delay: {updatedTiming.VisualDelay}",
                _ => $"Audio latency: {updatedTiming.AudioLatency}",
            };
        }

        if (osdLine != null)
        {
            Flash(osdLine);
        }
    }

    private static bool IsShiftHeld()
    {
        var input = Input.Singleton;
        return input.IsKeyPressed(Key.Shift);
    }

    /// <summary>Frees the OSD labels when the stage tears down.</summary>
    public void Dispose()
    {
        if (osd is not null && GodotObject.IsInstanceValid(osd))
        {
            osd.QueueFree();
        }
        if (status is not null && GodotObject.IsInstanceValid(status))
        {
            status.QueueFree();
        }
    }
}
