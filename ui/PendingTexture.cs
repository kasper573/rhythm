using Godot;

namespace Rhythm;

/// <summary>
/// A texture that may still be loading from disk. Call Poll() each frame
/// to check if it's ready.
/// </summary>
public class PendingTexture
{
    private string path;
    private Texture2D? texture;
    private bool attempted;

    private PendingTexture(string path)
    {
        this.path = path;
    }

    /// <summary>Start loading a texture from the given asset path.</summary>
    public static PendingTexture Load(string path) => new(path);

    /// <summary>
    /// Poll for completion. Returns null if still loading, or the texture
    /// (which may be null if the file didn't exist).
    /// </summary>
    public Texture2D? Poll()
    {
        if (texture is not null)
            return texture;

        if (attempted)
            return null;

        attempted = true;
        try
        {
            texture = GD.Load<Texture2D>(path);
        }
        catch
        {
            // File not found or couldn't load
        }

        return texture;
    }
}
