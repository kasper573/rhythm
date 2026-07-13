using Godot;
using Godot.Collections;
using Rhythm.Core;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class HealthBarConfig : Resource
{
    [Export] public RhythmCycle? Glow { get; set; }
    [Export] public RhythmCycle? Liquid { get; set; }
    [Export] public Array<HealthGradient> Colors { get; set; } = [];

    public HealthGradient GradientAt(Percent health)
    {
        if (Colors.Count == 0)
            throw new InvalidOperationException("HealthBarConfig.Colors is empty");

        for (int i = Colors.Count - 1; i >= 0; i--)
        {
            if (health.Value >= Colors[i].MinHealth)
            {
                return Colors[i];
            }
        }

        return Colors[0];
    }
}
