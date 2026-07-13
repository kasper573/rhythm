using Godot;

namespace Rhythm;

/// <summary>
/// An image decoding from the asset filesystem off the main thread. The
/// decode runs on a task; <see cref="Poll"/> returns null until it
/// finishes, then a <see cref="Loaded"/> once — carrying the texture, or a
/// null texture when the file was missing or unreadable. Owners poll every
/// frame and act on the first non-null result.
/// </summary>
public sealed class PendingTexture
{
    /// <summary>A finished decode: <see cref="Texture"/> is null when the load failed.</summary>
    public readonly record struct Loaded(Texture2D? Texture);

    private readonly Task<Image?> decode;
    private bool resolved;

    private PendingTexture(string path)
    {
        decode = Task.Run(() =>
        {
            var image = new Image();
            return image.Load(path) == Error.Ok ? image : null;
        });
    }

    /// <summary>Begins decoding the image at the given asset filesystem path.</summary>
    public static PendingTexture Load(string path) => new(path);

    /// <summary>
    /// The decode's outcome: null while it is still working, then a
    /// <see cref="Loaded"/> exactly once when it finishes.
    /// </summary>
    public Loaded? Poll()
    {
        if (resolved || !decode.IsCompleted)
        {
            return null;
        }

        resolved = true;
        var image = decode.Result;
        return new Loaded(image is not null ? ImageTexture.CreateFromImage(image) : null);
    }
}
