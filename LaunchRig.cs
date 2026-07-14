using System.Text.Json;
using System.Text.Json.Nodes;
using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The launch directives that run for the whole session: input automation,
/// the frame report, and the quit timer. Installed by <see cref="Launch.Boot"/>
/// when any of --pulse, --hold, --frame-report, or --quit-after-seconds are set.
/// </summary>
[GlobalClass]
public partial class LaunchRig : Node
{
    public (GameAction, double)? Pulse { get; set; }
    public GameAction? Hold { get; set; }
    public string? FrameReport { get; set; }
    public double? QuitAfter { get; set; }

    private bool held;
    private double pulseSince;
    private double elapsed;
    private List<double> frames = [];

    public override void _Process(double delta)
    {
        elapsed += delta;

        if (FrameReport is not null)
        {
            frames.Add(delta);
        }

        if (Hold is { } action && !held)
        {
            held = true;
            PressAction(action, pressed: true);
        }

        if (Pulse is not null)
        {
            var (pulseAction, interval) = Pulse.Value;
            pulseSince += delta;
            if (held)
            {
                PressAction(pulseAction, pressed: false);
                held = false;
            }

            if (pulseSince >= interval)
            {
                pulseSince -= interval;
                PressAction(pulseAction, pressed: true);
                held = true;
            }
        }

        if (QuitAfter is not null && elapsed >= QuitAfter)
        {
            GetTree().Quit();
        }
    }

    public override void _ExitTree()
    {
        if (FrameReport is not null)
        {
            var frameArray = new JsonArray();
            foreach (var frame in frames)
            {
                frameArray.Add(JsonValue.Create(frame));
            }

            var report = new JsonObject
            {
                ["debug_build"] = JsonValue.Create(OS.HasFeature("debug")),
                ["frames"] = frameArray,
            };
            System.IO.File.WriteAllText(FrameReport, report.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
        }
    }

    private static void PressAction(GameAction action, bool pressed)
    {
        try
        {
            var settings = Settings.Instance;
            var defaults = Config.Current.Defaults ?? throw new InvalidOperationException("Config.Defaults must not be null");
            var key = settings.Machine.Keymap.Key(action, defaults.ToKeymap());
            var keycode = OS.FindKeycodeFromString(key);

            var evt = new InputEventKey { PhysicalKeycode = keycode, Pressed = pressed };
            Input.Singleton.ParseInputEvent(evt);
        }
        catch (Exception ex)
        {
            GD.PrintErr($"Failed to press action {action}: {ex.Message}");
        }
    }
}
