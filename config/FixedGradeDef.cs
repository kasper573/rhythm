using Godot;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class FixedGradeDef : Resource
{
    [Export] public string Name { get; set; } = "";
    [Export] public Color Color { get; set; } = Colors.White;
    [Export] public int HealthOffset { get; set; }
    [Export] public int Points { get; set; }
    [Export] public Color GlowColor { get; set; } = Colors.White;
    [Export] public float GlowStrength { get; set; }
}
