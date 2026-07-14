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
    private static StepfileLibrary? instance;

    public static StepfileLibrary Instance =>
        instance ?? throw new InvalidOperationException("Library autoload not in tree");

    public override void _EnterTree()
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        instance = StepfileLibrary.Scan();
    }
}
