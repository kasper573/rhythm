using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The sound-effect autoload: every effect decoded once at boot, played
/// through a pool of players on the Sfx bus so overlapping cues mix.
/// </summary>
[GlobalClass]
public partial class SfxPlayer : Node
{
    private Dictionary<Sfx, AudioStream> streams = [];
    private List<AudioStreamPlayer> pool = [];

    public static SfxPlayer Instance { get; private set; } = null!;

    public override void _EnterTree()
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        Instance = this;

        streams = [];
        foreach (Sfx sfx in Enum.GetValues(typeof(Sfx)))
        {
            var path = Assets.Path($"sfx/{SfxName(sfx)}.wav");
            try
            {
                var bytes = System.IO.File.ReadAllBytes(path);
                var stream = AudioStreamWav.LoadFromBuffer(bytes);
                if (stream is not null)
                {
                    streams[sfx] = stream;
                }
                else
                {
                    GD.PushWarning($"sfx cannot decode: {path}");
                }
            }
            catch (Exception ex)
            {
                GD.PushWarning($"sfx unavailable: {path}: {ex.Message}");
            }
        }
    }

    public void Play(Sfx sfx)
    {
        if (!streams.TryGetValue(sfx, out var stream))
        {
            return;
        }

        var player = pool.Find(p => !p.Playing);
        if (player is null)
        {
            player = new AudioStreamPlayer
            {
                Bus = AudioBuses.Sfx,
            };
            AddChild(player);
            pool.Add(player);
        }

        player.Stream = stream;
        player.Play();
    }

    private static string SfxName(Sfx sfx) => sfx switch
    {
        Sfx.Navigate => "navigate",
        Sfx.Select => "select",
        Sfx.Cancel => "cancel",
        Sfx.WheelMove => "wheel_move",
        Sfx.WheelSelect => "wheel_select",
        Sfx.GroupToggle => "group_toggle",
        Sfx.StartFile => "start_file",
        Sfx.Tick => "tick",
        Sfx.Fail => "fail",
        _ => throw new ArgumentOutOfRangeException(nameof(sfx), sfx, null),
    };
}

/// <summary>Sound effects the game can play.</summary>
public enum Sfx
{
    Navigate,
    Select,
    Cancel,
    WheelMove,
    WheelSelect,
    GroupToggle,
    StartFile,
    Tick,
    Fail,
}

public static class SfxExtensions
{
    /// <summary>Plays the sound on the shared player pool.</summary>
    public static void Play(this Sfx sfx)
    {
        SfxPlayer.Instance.Play(sfx);
    }
}
