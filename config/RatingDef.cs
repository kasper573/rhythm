using Godot;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class RatingDef : Resource
{
    [Export] public string Image { get; set; } = "";
    [Export] public RuleKind RuleKind { get; set; }
    [Export] public int PointPercentage { get; set; }
    [Export] public string AllGradesGte { get; set; } = "";
}

public enum RuleKind
{
    PointPercentage,
    AllGradesGte,
}
