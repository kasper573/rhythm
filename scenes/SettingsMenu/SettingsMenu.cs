using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>Chooses which settings surface to open, or returns to the title.</summary>
[GlobalClass]
public partial class SettingsMenu : Control
{
    public override void _Ready()
    {
        Scenes.PlayDefaultBgm();
        Scenes.SpawnDefaultBackground(this);
        GetNode<Menu>("Menu").Selected += OnSelected;
    }

    public override void _Process(double delta)
    {
        if (Game.Instance.AcceptsInput && Actions.JustPressed(GameActions.Cancel(PlayerId.P1)))
        {
            SfxPlayer.Instance.Play(Sfx.Cancel);
            Game.Instance.ChangeScene(GameScene.MainMenu);
        }
    }

    private void OnSelected(int index) =>
        Game.Instance.ChangeScene(index == 0 ? GameScene.Keymap : GameScene.AudioSettings);
}
