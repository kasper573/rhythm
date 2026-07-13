using Godot;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class SpeedModifierSet : Resource
{
    [Export] public float[] Options { get; set; } = [];
    [Export] public float Default { get; set; }
}
