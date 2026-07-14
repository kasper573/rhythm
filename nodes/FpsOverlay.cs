using Godot;

namespace Rhythm;

/// <summary>
/// A frame-rate meter pinned to the bottom-right corner: the current FPS and
/// its observed range as text above a scrolling histogram of recent frames.
/// Hidden until the ToggleFps action shows it; the node listens for the
/// toggle itself.
/// </summary>
[Tool]
[GlobalClass]
public partial class FpsOverlay : Control
{
    private const float ReadoutSize = 13.0f;
    private const float PanelPadding = 4.0f;
    private const float GraphWidth = 120.0f;
    private const float GraphHeight = 34.0f;
    private const int Columns = 96;

    private Color fg = Colors.White;
    private Color bg = Colors.Black;
    private float edgePadding = 8.0f;
    private FpsHistory history = new();
    private float smoothed;
    private Label? readout;
    private ShaderMaterial? graph;

    [ExportGroup("Overlay")]
    [Export]
    public Color Fg
    {
        get => fg;
        set { fg = value; Rebuild(); }
    }

    [Export]
    public Color Bg
    {
        get => bg;
        set { bg = value; Rebuild(); }
    }

    [Export(PropertyHint.Range, "0,64,1")]
    public float EdgePadding
    {
        get => edgePadding;
        set { edgePadding = value; Rebuild(); }
    }

    public override void _Ready()
    {
        SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
        Visible = false;
        MouseFilter = MouseFilterEnum.Ignore;
        Rebuild();
    }

    public override void _Process(double delta)
    {
        if (Engine.IsEditorHint())
        {
            return;
        }

        if (Actions.JustPressed(Rhythm.Core.GameAction.ToggleFps))
        {
            Visible = !Visible;
        }

        if (!Visible || delta <= 0.0)
        {
            return;
        }

        var fps = (float)(1.0 / delta);
        smoothed += (fps - smoothed) * (smoothed == 0.0f ? 1.0f : 0.1f);
        history.Push(fps);

        var (low, high) = history.Range() ?? (fps, fps);

        if (readout is not null)
        {
            readout.Text = $"{smoothed:F0} FPS ({low:F0}-{high:F0})";
        }

        if (graph is not null)
        {
            var samples = new Godot.Collections.Array();
            foreach (var sample in history.Normalized())
            {
                samples.Add(sample);
            }
            graph.SetShaderParameter("samples", samples);
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

        var panel = new ColorRect { Color = bg };
        var column = new VBoxContainer();
        column.AddThemeConstantOverride("separation", 3);

        var readoutLabel = Text.Label("", ReadoutSize, fg);
        column.AddChild(readoutLabel);

        var shader = GD.Load<Shader>("res://nodes/fps_overlay.gdshader");
        var material = new ShaderMaterial { Shader = shader };
        material.SetShaderParameter("fg", fg);
        material.SetShaderParameter("bg", bg);

        var graphRect = new ColorRect
        {
            Material = material,
            CustomMinimumSize = new Vector2(GraphWidth, GraphHeight)
        };
        column.AddChild(graphRect);

        panel.AddChild(column);
        column.Position = new Vector2(PanelPadding, PanelPadding);
        AddChild(panel);

        var panelSize = new Vector2(
            GraphWidth + PanelPadding * 2.0f,
            ReadoutSize + 8.0f + GraphHeight + PanelPadding * 2.0f
        );
        panel.SetAnchorsPreset(Control.LayoutPreset.BottomRight);
        panel.SetOffset(Side.Left, -panelSize.X - edgePadding);
        panel.SetOffset(Side.Top, -panelSize.Y - edgePadding);
        panel.SetOffset(Side.Right, -edgePadding);
        panel.SetOffset(Side.Bottom, -edgePadding);

        readout = readoutLabel;
        graph = material;
    }

    private struct FpsHistory
    {
        private float[] ring;
        private int next;

        public FpsHistory()
        {
            ring = new float[Columns];
            next = 0;
        }

        public void Push(float fps)
        {
            ring[next] = fps;
            next = (next + 1) % Columns;
        }

        public float[] Normalized()
        {
            var peak = 0.0f;
            foreach (var f in ring)
            {
                if (f > peak)
                    peak = f;
            }
            peak = Mathf.Max(peak, 1.0f);

            var samples = new float[Columns];
            for (int i = 0; i < Columns; i++)
            {
                samples[i] = Mathf.Clamp(ring[(next + i) % Columns] / peak, 0.0f, 1.0f);
            }
            return samples;
        }

        public (float, float)? Range()
        {
            float? first = null;
            float low = float.MaxValue;
            float high = float.MinValue;

            foreach (var fps in ring)
            {
                if (fps > 0.0f)
                {
                    first ??= fps;
                    if (fps < low) low = fps;
                    if (fps > high) high = fps;
                }
            }

            return first is null ? null : (low, high);
        }
    }
}
