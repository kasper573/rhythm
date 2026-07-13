using Godot;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class HealthColorStop : Resource
{
    [Export(PropertyHint.Range, "0,100")] public float Percent { get; set; }
    [Export] public Color Color { get; set; } = Colors.White;
}
