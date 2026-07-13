using Godot;

namespace Rhythm;

/// <summary>The title screen: start a session, open settings, or quit.</summary>
[GlobalClass]
public partial class MainMenu : Control
{
    public override void _Ready()
    {
        Scenes.PlayDefaultBgm();
        Scenes.SpawnDefaultBackground(this);
        GetNode<Menu>("Menu").Selected += OnSelected;
    }

    private void OnSelected(int index)
    {
        switch (index)
        {
            case 0:
                Game.Instance.ChangeScene(GameScene.ModeSelect);
                break;
            case 1:
                Game.Instance.ChangeScene(GameScene.SettingsMenu);
                break;
            default:
                GetTree().Quit();
                break;
        }
    }
}
