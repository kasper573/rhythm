using Godot;
using Rhythm.Core;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class GradeDef : Resource
{
    [Export] public string Name { get; set; } = "";
    [Export] public float WindowMs { get; set; }
    [Export] public Color Color { get; set; } = Colors.White;
    [Export] public bool BreaksCombo { get; set; }
    [Export] public TimingFeedbackKind TimingFeedback { get; set; }
    [Export] public bool HasArrowFlash { get; set; }
    [Export] public Color ArrowFlash { get; set; } = Colors.White;
    [Export] public int HealthOffset { get; set; }
    [Export] public int Points { get; set; }
    [Export] public Color GlowColor { get; set; } = Colors.White;
    [Export] public float GlowStrength { get; set; }

    public Seconds Window => Seconds.FromMillis(WindowMs);
}

public enum TimingFeedbackKind
{
    Off,
    Sign,
    Millis,
}
