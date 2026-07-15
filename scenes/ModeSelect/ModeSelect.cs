using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>Picks the play mode, then opens the song wheel.</summary>
[Tool]
[GlobalClass]
public partial class ModeSelect : Control
{
    public override void _Ready()
    {
        // The mode list is derived from the enum in both editor and game, so
        // the menu previews the real modes without a second source of truth.
        var menu = GetNode<Menu>("Menu");
        menu.Items = Enum.GetValues<PlayMode>().Select(mode => mode.ToString()).ToArray();

        if (Engine.IsEditorHint())
        {
            return;
        }

        Scenes.PlayDefaultBgm();
        Scenes.SpawnDefaultBackground(this);
        menu.Selected += OnSelected;
    }

    public override void _Process(double delta)
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        if (Game.Instance.AcceptsInput && Actions.JustPressed(GameActions.Cancel(PlayerId.P1)))
        {
            SfxPlayer.Instance.Play(Sfx.Cancel);
            Game.Instance.ChangeScene(GameScene.MainMenu);
        }
    }

    private void OnSelected(int index)
    {
        var modes = Enum.GetValues<PlayMode>();
        if (index < 0 || index >= modes.Length)
        {
            return;
        }

        Game.Instance.PlayMode = modes[index];
        Game.Instance.ChangeScene(GameScene.StepfileSelect);
    }
}
