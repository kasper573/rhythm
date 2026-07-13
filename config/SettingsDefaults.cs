using Godot;
using Godot.Collections;
using Rhythm.Core;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class SettingsDefaults : Resource
{
    [ExportGroup("Keymap")]
    [Export] public Dictionary KeymapBindings { get; set; } = [];

    [ExportGroup("Player Options")]
    [Export] public string NoteSkin { get; set; } = "";
    [Export] public NoteSpeedKind NoteSpeedKind { get; set; }
    [Export(PropertyHint.Range, "0.01,10")] public float NoteSpeedValue { get; set; } = 1;
    [Export] public Perspective Perspective { get; set; }
    [Export] public GradeLayer GradeLayer { get; set; }
    [Export(PropertyHint.Range, "0,100")] public float GradePositionPercent { get; set; }

    [ExportGroup("Timing")]
    [Export(PropertyHint.Range, "-500,500")] public int MachineOffsetMs { get; set; }
    [Export(PropertyHint.Range, "-500,500")] public int VisualDelayMs { get; set; }
    [Export] public bool HasAudioLatency { get; set; }
    [Export(PropertyHint.Range, "-500,500")] public int AudioLatencyMs { get; set; }

    [ExportGroup("Volume")]
    [Export(PropertyHint.Range, "0,2")] public float MasterVolume { get; set; } = 1;
    [Export(PropertyHint.Range, "0,2")] public float SfxVolume { get; set; } = 1;
    [Export(PropertyHint.Range, "0,2")] public float MusicVolume { get; set; } = 1;

    public Keymap ToKeymap()
    {
        var bindings = new System.Collections.Generic.Dictionary<GameAction, string>();
        foreach (var key in KeymapBindings.Keys)
        {
            if (System.Enum.TryParse<GameAction>(key.ToString(), out var action))
            {
                bindings[action] = KeymapBindings[key].ToString();
            }
        }

        return new Keymap(bindings);
    }

    public PlayerOptions ToPlayerOptions()
    {
        var noteSpeed = NoteSpeedKind == NoteSpeedKind.Constant
            ? (NoteSpeed)new NoteSpeed.Constant(NoteSpeedValue)
            : new NoteSpeed.Dynamic(NoteSpeedValue);

        return new PlayerOptions
        {
            NoteSkin = NoteSkin,
            NoteSpeed = noteSpeed,
            Perspective = Perspective,
            GradeLayer = GradeLayer,
            GradePosition = new Percent(GradePositionPercent),
        };
    }

    public TimingSettings ToTimingSettings()
    {
        return new TimingSettings
        {
            MachineOffset = new Millis(MachineOffsetMs),
            VisualDelay = new Millis(VisualDelayMs),
            AudioLatency = HasAudioLatency ? new Millis(AudioLatencyMs) : null,
        };
    }

    public VolumeSettings ToVolumeSettings()
    {
        return new VolumeSettings(MasterVolume, SfxVolume, MusicVolume);
    }
}

public enum NoteSpeedKind
{
    Constant,
    Dynamic,
}
