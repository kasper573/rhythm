using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// Resolves and installs the runtime asset root before anything reads it —
/// the first autoload. The assets live in an <c>assets/</c> folder beside
/// the executable in an export, and beside <c>project.godot</c> in the
/// editor; either way they are loaded from the filesystem, never packed.
/// </summary>
[GlobalClass]
public partial class Boot : Node
{
    public override void _EnterTree()
    {
        var baseDir = OS.HasFeature("editor")
            ? ProjectSettings.GlobalizePath("res://")
            : OS.GetExecutablePath().GetBaseDir();
        Assets.Install(System.IO.Path.Combine(baseDir, "assets"));
    }
}
