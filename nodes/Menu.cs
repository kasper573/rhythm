using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// A full-screen titled menu: a centered title over a highlighted list of
/// items. It drives itself from the shared <see cref="NavInput"/> — P1's
/// Up/Down step pulses move the highlight, P1's Select fires
/// <see cref="SelectedEventHandler"/> with the active index. An owner that
/// drives the highlight itself (a keymap editor must stay operable however
/// broken the bindings are) calls <see cref="SetOwnerDriven"/>, leaving only
/// the highlight to this node. Configure <see cref="Title"/> and
/// <see cref="Items"/> in the inspector; the menu renders them live.
/// </summary>
[Tool]
[GlobalClass]
public partial class Menu : Control
{
    private string title = string.Empty;
    private string[] items = [];
    private int active;
    private bool ownerDriven;
    private readonly List<Label> labels = [];

    [Export(PropertyHint.MultilineText)]
    public string Title
    {
        get => title;
        set { title = value; Rebuild(); }
    }

    [Export]
    public string[] Items
    {
        get => items;
        set { items = value; Rebuild(); }
    }

    [Signal]
    public delegate void SelectedEventHandler(int index);

    /// <summary>The highlighted item.</summary>
    public int Active => active;

    public int Count => items.Length;

    /// <summary>Moves the highlight directly — the owner-driven path.</summary>
    public void SetActive(int index)
    {
        active = index;
        RefreshHighlight();
    }

    public void SetOwnerDriven() => ownerDriven = true;

    public override void _Ready()
    {
        SetAnchorsAndOffsetsPreset(LayoutPreset.FullRect);
        Rebuild();
    }

    public override void _Process(double delta)
    {
        if (Engine.IsEditorHint() || ownerDriven || !NavInput.Instance.Active)
        {
            return;
        }

        foreach (var pulse in NavInput.Instance.Pulses)
        {
            if (pulse.AsStep() is (PlayerId.P1, var direction))
            {
                if (direction == StepDirection.Up)
                {
                    Step(back: true);
                }
                else if (direction == StepDirection.Down)
                {
                    Step(back: false);
                }
            }
        }

        if (Count > 0 && Actions.JustPressed(GameActions.Select(PlayerId.P1)))
        {
            SfxPlayer.Instance.Play(Sfx.Select);
            EmitSignal(SignalName.Selected, active);
        }
    }

    private void Rebuild()
    {
        if (!IsInsideTree())
        {
            return;
        }

        foreach (var child in GetChildren())
        {
            child.QueueFree();
        }

        labels.Clear();
        active = 0;

        var center = new CenterContainer();
        center.SetAnchorsAndOffsetsPreset(LayoutPreset.FullRect);
        var column = new VBoxContainer { Alignment = BoxContainer.AlignmentMode.Center };
        column.AddThemeConstantOverride("separation", 12);

        var heading = Text.Label(title, 52.0f, Screen.TitleColor);
        heading.HorizontalAlignment = HorizontalAlignment.Center;
        column.AddChild(heading);
        column.AddChild(new Control { CustomMinimumSize = new Vector2(0.0f, 32.0f) });

        for (var index = 0; index < items.Length; index++)
        {
            var item = Text.Label(items[index], 34.0f, index == 0 ? Screen.ActiveColor : Screen.InactiveColor);
            item.HorizontalAlignment = HorizontalAlignment.Center;
            column.AddChild(item);
            labels.Add(item);
        }

        center.AddChild(column);
        AddChild(center);
    }

    private void Step(bool back)
    {
        if (items.Length == 0)
        {
            return;
        }

        active = back
            ? ((active + items.Length - 1) % items.Length)
            : ((active + 1) % items.Length);
        RefreshHighlight();
        SfxPlayer.Instance.Play(Sfx.Navigate);
    }

    private void RefreshHighlight()
    {
        for (var index = 0; index < labels.Count; index++)
        {
            labels[index].AddThemeColorOverride("font_color", index == active ? Screen.ActiveColor : Screen.InactiveColor);
        }
    }
}
