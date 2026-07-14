using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// The keymap editor. Its own navigation resolves through the config's
/// DEFAULT keymap, never the live one it edits: however broken the stored
/// bindings get, this scene stays operable to repair them.
/// </summary>
[GlobalClass]
public partial class KeymapScene : Control
{
    private int active;
    private GameAction? prompt;
    private bool promptJustClosed;
    private bool cancelArmed;
    private double cancelHeld;
    private bool[] navBefore = [false, false, false, false];
    private List<Label> actionLabels = [];
    private List<Label> bindingLabels = [];
    private VBoxContainer actionColumn = null!;
    private VBoxContainer keyColumn = null!;
    private Label promptLabel = null!;
    private Label helpLabel = null!;
    private Keymap defaultKeymap = null!;

    private const double RESET_HOLD = 0.5;

    private static readonly GameAction[] NAV_ACTIONS =
    [
        GameAction.P1Up,
        GameAction.P1Down,
        GameAction.P1Select,
        GameAction.P1Cancel,
    ];

    public override void _Ready()
    {
        Scenes.PlayDefaultBgm();
        Scenes.SpawnDefaultBackground(this);

        defaultKeymap = Config.Current.Defaults!.ToKeymap();

        actionColumn = GetNode<VBoxContainer>("%ActionColumn");
        keyColumn = GetNode<VBoxContainer>("%KeyColumn");
        promptLabel = GetNode<Label>("%Prompt");
        helpLabel = GetNode<Label>("%Help");

        foreach (var action in GameActions.All)
        {
            var name = Text.Label(action.Label(), 19.0f, Screen.InactiveColor);
            actionColumn.AddChild(name);
            actionLabels.Add(name);

            var key = Text.Label(KeyLabel(action, defaultKeymap), 19.0f, Screen.InactiveColor);
            keyColumn.AddChild(key);
            bindingLabels.Add(key);
        }

        UpdateHelpText();
        RefreshRows();
    }

    public override void _Input(InputEvent @event)
    {
        if (prompt is null)
        {
            return;
        }

        var keyEvent = @event as InputEventKey;
        if (keyEvent is null)
        {
            return;
        }

        if (!keyEvent.Pressed || keyEvent.Echo)
        {
            return;
        }

        var key = keyEvent.PhysicalKeycode;
        var cancelKey = DefaultKey(GameAction.P1Cancel);

        if (key == cancelKey)
        {
            SfxPlayer.Instance.Play(Sfx.Cancel);
        }
        else
        {
            Settings.Instance.EditMachine(m =>
            {
                m.Keymap.Set(prompt.Value, Actions.KeyName(key));
                return m;
            });
            SfxPlayer.Instance.Play(Sfx.Select);
        }

        prompt = null;
        promptJustClosed = true;
        RefreshPrompt();
        RefreshRows();
    }

    public override void _Process(double delta)
    {
        var input = Input.Singleton;

        var now = new bool[4];
        for (int i = 0; i < NAV_ACTIONS.Length; i++)
        {
            now[i] = input.IsPhysicalKeyPressed(DefaultKey(NAV_ACTIONS[i]));
        }

        var just = (int index) => now[index] && !navBefore[index];
        var released = (int index) => !now[index] && navBefore[index];

        navBefore = now;

        if (!Game.Instance.AcceptsInput)
        {
            return;
        }

        if (promptJustClosed)
        {
            promptJustClosed = false;
            return;
        }

        if (prompt is not null)
        {
            return;
        }

        if (just(0))
        {
            Navigate(true);
        }

        if (just(1))
        {
            Navigate(false);
        }

        if (just(2))
        {
            OpenPrompt();
            return;
        }

        CancelGesture(delta, just(3), released(3));
    }

    private void Navigate(bool back)
    {
        var len = GameActions.All.Count;
        active = back
            ? (active + len - 1) % len
            : (active + 1) % len;
        SfxPlayer.Instance.Play(Sfx.Navigate);
        RefreshRows();
    }

    private void OpenPrompt()
    {
        if (active >= 0 && active < GameActions.All.Count)
        {
            prompt = GameActions.All[active];
        }
        SfxPlayer.Instance.Play(Sfx.Select);
        RefreshPrompt();
    }

    private void RefreshRows()
    {
        var keymap = Settings.Instance.Machine.Keymap;
        for (int index = 0; index < GameActions.All.Count; index++)
        {
            var action = GameActions.All[index];
            var color = index == active ? Colors.White : Screen.InactiveColor;
            actionLabels[index].AddThemeColorOverride("font_color", color);
            bindingLabels[index].Text = KeyLabel(action, keymap);
            bindingLabels[index].AddThemeColorOverride("font_color", color);
        }
    }

    private void RefreshPrompt()
    {
        var text = prompt switch
        {
            null => "",
            GameAction action => $"Press a key for \"{action.Label()}\" ({Actions.KeyName(DefaultKey(GameAction.P1Cancel))} aborts)",
        };

        promptLabel.Text = text;
    }

    private void UpdateHelpText()
    {
        var resetKey = DefaultKey(GameAction.P1Cancel);
        helpLabel.Text = $"Hold {Actions.KeyName(resetKey)} to reset selected key to default";
    }

    private void CancelGesture(double delta, bool justPressed, bool justReleased)
    {
        if (justPressed)
        {
            cancelArmed = true;
            cancelHeld = 0.0;
        }

        if (!cancelArmed)
        {
            return;
        }

        var cancelKey = DefaultKey(GameAction.P1Cancel);
        if (Input.Singleton.IsPhysicalKeyPressed(cancelKey))
        {
            cancelHeld += delta;
            if (cancelHeld >= RESET_HOLD)
            {
                cancelArmed = false;
                if (active >= 0 && active < GameActions.All.Count)
                {
                    var action = GameActions.All[active];
                    Settings.Instance.EditMachine(m =>
                    {
                        m.Keymap.Reset(action);
                        return m;
                    });
                    SfxPlayer.Instance.Play(Sfx.Select);
                    RefreshRows();
                }
            }
            return;
        }

        if (justReleased)
        {
            cancelArmed = false;
            SfxPlayer.Instance.Play(Sfx.Cancel);
            Game.Instance.ChangeScene(GameScene.SettingsMenu);
        }
    }

    private Key DefaultKey(GameAction action)
    {
        var keyName = defaultKeymap.Binding(action)
            ?? throw new InvalidOperationException($"default keymap must bind {action}");
        return Actions.KeyFromName(keyName);
    }

    private string KeyLabel(GameAction action, Keymap keymap)
    {
        var keyName = keymap.Key(action, defaultKeymap);
        return Actions.KeyName(Actions.KeyFromName(keyName));
    }
}
