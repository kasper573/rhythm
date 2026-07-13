using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The user settings autoload: the machine's own settings (keymap, timing
/// calibration, volumes) and each player slot's presentation options. Any
/// edit is applied on the spot — input map, audio buses — persisted at the
/// end of the frame, and announced through the Changed signal.
/// </summary>
[GlobalClass]
public partial class Settings : Node
{
    private MachineSettings machine = new()
    {
        Keymap = new Keymap(),
        Timing = new TimingSettings
        {
            MachineOffset = new Millis(0),
            VisualDelay = new Millis(0),
            AudioLatency = null,
        },
        Volume = new VolumeSettings(0f, 0f, 0f),
    };

    private PerPlayer<PlayerOptions> players = new(
        new PlayerOptions
        {
            NoteSkin = "",
            NoteSpeed = new NoteSpeed.Dynamic(1f),
            Perspective = Perspective.None,
            GradeLayer = GradeLayer.Behind,
            GradePosition = new Percent(50f),
        },
        new PlayerOptions
        {
            NoteSkin = "",
            NoteSpeed = new NoteSpeed.Dynamic(1f),
            Perspective = Perspective.None,
            GradeLayer = GradeLayer.Behind,
            GradePosition = new Percent(50f),
        });

    private bool dirtyMachine;
    private bool dirtyPlayers;

    [Signal]
    public delegate void ChangedEventHandler();

    public static Settings Instance { get; private set; } = null!;

    public MachineSettings Machine => machine;

    public PlayerOptions Player(PlayerId player) => players[player];

    public PerPlayer<PlayerOptions> Players => players;

    public override void _EnterTree()
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        Instance = this;

        var defaults = Config.Current.Defaults
            ?? throw new InvalidOperationException("Config.Defaults must not be null");

        machine = LoadMachineSettings(defaults);
        players = LoadPlayerSettings(defaults);

        AudioBuses.Ensure();
        ApplyMachine();
    }

    public override void _Process(double delta)
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        if (dirtyMachine)
        {
            dirtyMachine = false;
            Persistence.Save("machine_settings.json", new MachineSettingsFile
            {
                Keymap = machine.Keymap,
                Timing = new TimingSettingsFile
                {
                    MachineOffset = machine.Timing.MachineOffset,
                    VisualDelay = machine.Timing.VisualDelay,
                    AudioLatency = machine.Timing.AudioLatency,
                },
                Volume = new VolumeSettingsFile
                {
                    Master = machine.Volume.Master,
                    Music = machine.Volume.Music,
                    Sfx = machine.Volume.Sfx,
                },
            });
        }

        if (dirtyPlayers)
        {
            dirtyPlayers = false;
            foreach (var player in new[] { PlayerId.P1, PlayerId.P2 })
            {
                Persistence.Save(PlayerSettingsFile(player), new PlayerOptionsFile
                {
                    NoteSkin = players[player].NoteSkin,
                    NoteSpeed = players[player].NoteSpeed,
                    Perspective = players[player].Perspective,
                    GradeLayer = players[player].GradeLayer,
                    GradePosition = players[player].GradePosition,
                });
            }
        }
    }

    public void EditMachine(System.Func<MachineSettings, MachineSettings> edit)
    {
        machine = edit(machine);
        dirtyMachine = true;
        ApplyMachine();
        EmitSignal(SignalName.Changed);
    }

    public void EditPlayer(PlayerId player, System.Func<PlayerOptions, PlayerOptions> edit)
    {
        players[player] = edit(players[player]);
        dirtyPlayers = true;
        EmitSignal(SignalName.Changed);
    }

    private void ApplyMachine()
    {
        Actions.Apply(machine.Keymap, Config.Current.Defaults?.ToKeymap() ?? new Keymap());
        AudioBuses.ApplyVolumes(machine.Volume);
    }

    private static MachineSettings LoadMachineSettings(SettingsDefaults defaults)
    {
        var file = Persistence.Load<MachineSettingsFile>("machine_settings.json");
        return new MachineSettings
        {
            Keymap = file.Keymap ?? new Keymap(),
            Timing = new TimingSettings
            {
                MachineOffset = file.Timing?.MachineOffset ?? defaults.ToTimingSettings().MachineOffset,
                VisualDelay = file.Timing?.VisualDelay ?? defaults.ToTimingSettings().VisualDelay,
                AudioLatency = file.Timing?.AudioLatency ?? defaults.ToTimingSettings().AudioLatency,
            },
            Volume = new VolumeSettings(
                file.Volume?.Master ?? defaults.ToVolumeSettings().Master,
                file.Volume?.Sfx ?? defaults.ToVolumeSettings().Sfx,
                file.Volume?.Music ?? defaults.ToVolumeSettings().Music),
        };
    }

    private static PerPlayer<PlayerOptions> LoadPlayerSettings(SettingsDefaults defaults)
    {
        var load = (PlayerId player) =>
        {
            var file = Persistence.Load<PlayerOptionsFile>(PlayerSettingsFile(player));
            var defaultOptions = defaults.ToPlayerOptions();
            return new PlayerOptions
            {
                NoteSkin = file.NoteSkin ?? defaultOptions.NoteSkin,
                NoteSpeed = file.NoteSpeed ?? defaultOptions.NoteSpeed,
                Perspective = file.Perspective ?? defaultOptions.Perspective,
                GradeLayer = file.GradeLayer ?? defaultOptions.GradeLayer,
                GradePosition = file.GradePosition ?? defaultOptions.GradePosition,
            };
        };

        return new PerPlayer<PlayerOptions>(load(PlayerId.P1), load(PlayerId.P2));
    }

    private static string PlayerSettingsFile(PlayerId player) => player switch
    {
        PlayerId.P1 => "p1_settings.json",
        PlayerId.P2 => "p2_settings.json",
        _ => throw new ArgumentOutOfRangeException(nameof(player)),
    };

    #region Serialization DTOs

    [System.Serializable]
    public sealed class MachineSettingsFile
    {
        [System.Text.Json.Serialization.JsonPropertyName("keymap")]
        public Keymap? Keymap { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("timing")]
        public TimingSettingsFile? Timing { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("volume")]
        public VolumeSettingsFile? Volume { get; set; }
    }

    [System.Serializable]
    public sealed class TimingSettingsFile
    {
        [System.Text.Json.Serialization.JsonPropertyName("machine_offset")]
        public Millis? MachineOffset { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("visual_delay")]
        public Millis? VisualDelay { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("audio_latency")]
        public Millis? AudioLatency { get; set; }
    }

    [System.Serializable]
    public sealed class VolumeSettingsFile
    {
        [System.Text.Json.Serialization.JsonPropertyName("master")]
        public float? Master { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("sfx")]
        public float? Sfx { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("music")]
        public float? Music { get; set; }
    }

    [System.Serializable]
    public sealed class PlayerOptionsFile
    {
        [System.Text.Json.Serialization.JsonPropertyName("note_skin")]
        public string? NoteSkin { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("note_speed")]
        public NoteSpeed? NoteSpeed { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("perspective")]
        public Perspective? Perspective { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("grade_layer")]
        public GradeLayer? GradeLayer { get; set; }

        [System.Text.Json.Serialization.JsonPropertyName("grade_position")]
        public Percent? GradePosition { get; set; }
    }

    #endregion
}
