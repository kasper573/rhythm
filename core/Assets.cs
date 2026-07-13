namespace Rhythm.Core;

/// <summary>
/// The folder the game's data is loaded from at runtime: the stepfile
/// library, note skins, fonts, sounds, and config. Installed once at boot
/// with the resolved absolute path (beside the executable in an export,
/// the project directory in the editor), then read everywhere.
/// </summary>
public static class Assets
{
    private static string? root;

    public static string Root =>
        root ?? throw new InvalidOperationException("asset root installed at boot");

    public static void Install(string path) => root = path;

    public static string Path(string relative) => System.IO.Path.Combine(Root, relative);

    /// <summary>
    /// The relative, forward-slashed form of a path under the asset root,
    /// or <c>null</c> if it lies outside — the shape log messages want.
    /// </summary>
    public static string? RelativePath(string absolute)
    {
        var relative = System.IO.Path.GetRelativePath(Root, absolute);
        if (relative.StartsWith("..", StringComparison.Ordinal) || System.IO.Path.IsPathRooted(relative))
        {
            return null;
        }

        return relative.Replace('\\', '/');
    }
}
