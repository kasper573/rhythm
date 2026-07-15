using Godot;
using Rhythm.Core;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class StageConfig : Resource
{
    [Export(PropertyHint.Range, "1,1000")] public int MaxArrowSize { get; set; } = 128;
    [Export(PropertyHint.Range, "0,1000")] public int MarginX { get; set; }
    [Export(PropertyHint.Range, "0,10")] public float FieldGapColumns { get; set; }
    [Export(PropertyHint.Range, "0,10")] public float LeadInSeconds { get; set; }

    /// <summary>How long the play scene holds on the finished chart before scoring.</summary>
    [Export(PropertyHint.Range, "0,10")] public float EndDelaySeconds { get; set; } = 2.0f;
    [Export(PropertyHint.Range, "0,500")] public int ScreenEdgePadding { get; set; }

    public Seconds LeadIn => new(LeadInSeconds);
    public Seconds EndDelay => new(EndDelaySeconds);
}
