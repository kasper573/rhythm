using Godot;
using Godot.Collections;
using Rhythm.Core;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class GradingConfig : Resource
{
    [Export(PropertyHint.Range, "0.01,10")] public float HoldGraceSeconds { get; set; } = 0.25f;
    [Export(PropertyHint.Range, "0.01,10")] public float RollGraceSeconds { get; set; } = 0.5f;
    [Export] public Array<GradeDef> Dynamic { get; set; } = [];
    [Export] public FixedGradeDef? Miss { get; set; }
    [Export] public FixedGradeDef? Ok { get; set; }
    [Export] public FixedGradeDef? Ng { get; set; }

    public Seconds HoldGrace => new(HoldGraceSeconds);
    public Seconds RollGrace => new(RollGraceSeconds);
}
