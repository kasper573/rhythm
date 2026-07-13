using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The scanned stepfile library autoload: holds the single StepfileLibrary
/// instance scanned once at boot. All scenes that reference stepfile data
/// (wheel rows, music player defaults, etc.) read from this instance, never
/// re-scan.
/// </summary>
[GlobalClass]
public partial class Library : Node
{
    private StepfileLibrary? library;

    public static StepfileLibrary Instance { get; private set; } = null!;

    public override void _EnterTree()
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        library = StepfileLibrary.Scan();
        Instance = library;
    }
}
