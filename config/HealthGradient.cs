using Godot;
using Godot.Collections;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class HealthGradient : Resource
{
    [Export(PropertyHint.Range, "0,100")] public float MinHealth { get; set; }
    [Export] public Array<HealthColorStop> Stops { get; set; } = [];
}
