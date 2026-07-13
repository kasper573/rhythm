using System.Text.Json;
using System.Text.Json.Serialization;

namespace Rhythm;

/// <summary>
/// Reads and writes the game's <c>user://</c> JSON files. On-disk fields are
/// optional so a file written by an older version still loads — absent
/// fields fall back to the config defaults at the call site — and a missing
/// or unreadable file loads as the type's default.
/// </summary>
public static class Persistence
{
    public static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        DictionaryKeyPolicy = null,
        WriteIndented = true,
        Converters = { new JsonStringEnumConverter() },
    };

    public static T Load<T>(string file)
        where T : new()
    {
        var text = ReadText(file);
        if (text is null)
        {
            return new T();
        }

        try
        {
            return JsonSerializer.Deserialize<T>(text, Options) ?? new T();
        }
        catch (JsonException)
        {
            return new T();
        }
    }

    public static void Save<T>(string file, T value)
    {
        using var handle = Godot.FileAccess.Open($"user://{file}", Godot.FileAccess.ModeFlags.Write);
        handle?.StoreString(JsonSerializer.Serialize(value, Options));
    }

    private static string? ReadText(string file)
    {
        using var handle = Godot.FileAccess.Open($"user://{file}", Godot.FileAccess.ModeFlags.Read);
        return handle?.GetAsText();
    }
}
