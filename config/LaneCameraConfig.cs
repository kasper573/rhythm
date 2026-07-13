using Godot;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class LaneCameraConfig : Resource
{
    [Export(PropertyHint.Range, "0.1,179.9")] public float FovDegrees { get; set; } = 45;
    [Export(PropertyHint.Range, "0,89.9")] public float TiltDegrees { get; set; } = 30;
}
