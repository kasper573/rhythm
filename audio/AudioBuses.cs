using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The game's audio buses. Music and sound effects each get their own so
/// the volume settings apply live to everything playing on them.
/// </summary>
public static class AudioBuses
{
    public const string Music = "Music";
    public const string Sfx = "Sfx";

    /// <summary>Creates the game's buses once; safe to call again.</summary>
    public static void Ensure()
    {
        var server = AudioServer.Singleton;
        foreach (var name in new[] { Music, Sfx })
        {
            if (server.GetBusIndex(name) < 0)
            {
                var index = server.BusCount;
                server.AddBus();
                server.SetBusName(index, name);
            }
        }
    }

    /// <summary>
    /// Applies the volume settings to the buses: <c>master</c> on the master
    /// bus, the rest on their own.
    /// </summary>
    public static void ApplyVolumes(VolumeSettings volume)
    {
        Set("Master", volume.Master);
        Set(Music, volume.Music);
        Set(Sfx, volume.Sfx);

        static void Set(string name, float linear)
        {
            var server = AudioServer.Singleton;
            var index = server.GetBusIndex(name);
            if (index >= 0)
            {
                server.SetBusVolumeDb(index, Mathf.LinearToDb(linear));
            }
        }
    }
}
