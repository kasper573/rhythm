using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// An isolated preview of the health vial over split bright/dark backgrounds,
/// driving its fill and beat so the liquid level, surface waves, gradient
/// scroll, and glow pulse all animate for review. Reachable directly for
/// capture; not part of the game flow.
/// </summary>
[GlobalClass]
public partial class VialDemo : Control
{
    private HealthVial vial = null!;
    private double clock;

    public override void _Ready()
    {
        vial = GetNode<HealthVial>("%Vial");
    }

    public override void _Process(double delta)
    {
        clock += delta;
        var fill = 0.55f + 0.4f * Mathf.Sin((float)clock * 0.7f);
        vial.SetFill(fill);
        vial.SetBeat(new Beat(clock * 2.0));
    }
}
