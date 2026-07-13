using Godot;
using Rhythm.Core;

namespace Rhythm;

[Tool]
[GlobalClass]
public partial class SpeedModifiers : Resource
{
    [Export] public SpeedModifierSet? Constant { get; set; }
    [Export] public SpeedModifierSet? Dynamic { get; set; }

    public SpeedModifierSet Set(NoteSpeed noteSpeed)
    {
        return noteSpeed switch
        {
            NoteSpeed.Constant => Constant ?? throw new InvalidOperationException("Constant speed modifier set is null"),
            NoteSpeed.Dynamic => Dynamic ?? throw new InvalidOperationException("Dynamic speed modifier set is null"),
            _ => throw new InvalidOperationException($"Unknown NoteSpeed type: {noteSpeed.GetType().Name}"),
        };
    }
}
