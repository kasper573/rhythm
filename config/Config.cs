using Godot;

namespace Rhythm;

[GlobalClass]
public partial class Config : Node
{
    private static GameConfig? cached;

    /// <summary>
    /// The loaded game config, read lazily from its <c>.tres</c> on first use
    /// so it serves editor <c>[Tool]</c> previews as well as the running game.
    /// </summary>
    public static GameConfig Current => cached ??= Load();

    private static GameConfig Load()
    {
        var config = GD.Load<GameConfig>("res://config/GameConfig.tres")
            ?? throw new InvalidOperationException("Failed to load config from res://config/GameConfig.tres");

        config.Validate();
        return config;
    }

    public override void _EnterTree()
    {
        if (!Engine.IsEditorHint())
        {
            // Fail fast at boot rather than mid-scene on first access.
            _ = Current;
        }
    }
}
