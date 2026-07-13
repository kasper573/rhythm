using Godot;

namespace Rhythm;

[GlobalClass]
public partial class Config : Node
{
    private static GameConfig? cached;

    public static GameConfig Current
    {
        get => cached ?? throw new InvalidOperationException("Config not loaded");
    }

    public static GameConfig Load()
    {
        if (cached != null)
            return cached;

        if (!Engine.IsEditorHint())
        {
            cached = GD.Load<GameConfig>("res://config/game_config.tres")
                ?? throw new InvalidOperationException("Failed to load config from res://config/game_config.tres");

            cached.Validate();
            return cached;
        }

        throw new InvalidOperationException("Config.Load() called in editor");
    }

    public override void _EnterTree()
    {
        if (!Engine.IsEditorHint())
        {
            cached = Load();
        }
    }
}
