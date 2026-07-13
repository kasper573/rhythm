using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// One player's note field: its own little 3D scene — receptors, notes,
/// holds, mines, and transient effects — rendered by a perspective camera
/// into a transparent viewport composited into the stage. The camera hovers
/// over the field's center where a flat view reproduces the canvas 1:1 on
/// the lane plane, with its frustum window shifted back over the canvas rect,
/// so every field keeps its own vanishing point; the player's perspective
/// pitches it around the receptor row.
/// </summary>
public sealed class NoteFieldRig
{
    public const float HoldOkFadeSeconds = 0.05f;
    private const float MineExplosionSeconds = 0.4f;
    private const float PressSeconds = 0.25f;

    /// <summary>The body slides this far under the cap, blending the cap's filtered top edge into the body.</summary>
    private const float BodyCapOverlap = 1.0f;

    /// <summary>Notes spawn far off-screen and are placed by the scroll from their first frame.</summary>
    private const float OffscreenY = -10_000.0f;

    private readonly SubViewport viewport;
    private readonly TextureRect display;
    private readonly Camera3D camera;
    private readonly Node3D space;
    private readonly float fov;
    private readonly float tilt;
    private readonly Perspective perspective;
    private readonly List<ReceptorEl> receptors = [];
    private readonly List<NoteEl> notes = [];
    private readonly List<MineEl> mines = [];
    private readonly List<FadingElement> fades = [];
    private Vector2 canvas;
    private double elapsed;

    private NoteFieldRig(FieldLayout layout, NoteSkin skin, Perspective perspective, float fov, float tilt,
        Vector2 canvas, SubViewport viewport, TextureRect display, Camera3D camera, Node3D space)
    {
        Layout = layout;
        Skin = skin;
        this.perspective = perspective;
        this.fov = fov;
        this.tilt = tilt;
        this.canvas = canvas;
        this.viewport = viewport;
        this.display = display;
        this.camera = camera;
        this.space = space;
    }

    public FieldLayout Layout { get; private set; }
    public NoteSkin Skin { get; }

    /// <summary>Builds an empty field into <paramref name="parent"/>: the viewport, its camera, and the receptor row.</summary>
    public static NoteFieldRig Build(Node parent, FieldLayout layout, NoteSkin skin, Perspective perspective,
        float fovDegrees, float tiltDegrees, Vector2 canvas)
    {
        var viewport = new SubViewport
        {
            TransparentBg = true,
            Size = new Vector2I((int)canvas.X, (int)canvas.Y),
            Msaa3D = Viewport.Msaa.Msaa4X,
            RenderTargetUpdateMode = SubViewport.UpdateMode.Always,
            OwnWorld3D = true,
        };
        var space = new Node3D();
        viewport.AddChild(space);
        var camera = new Camera3D { Projection = Camera3D.ProjectionType.Frustum, Current = true };
        space.AddChild(camera);
        parent.AddChild(viewport);

        var display = new TextureRect
        {
            StretchMode = TextureRect.StretchModeEnum.Scale,
            ExpandMode = TextureRect.ExpandModeEnum.IgnoreSize,
            MouseFilter = Control.MouseFilterEnum.Ignore,
            Texture = viewport.GetTexture(),
        };
        display.SetAnchorsAndOffsetsPreset(Control.LayoutPreset.FullRect);
        parent.AddChild(display);

        var rig = new NoteFieldRig(layout, skin, perspective, Mathf.DegToRad(fovDegrees), Mathf.DegToRad(tiltDegrees),
            canvas, viewport, display, camera, space);
        rig.SpawnReceptors();
        rig.SyncCamera(NoteField.TargetY);
        return rig;
    }

    /// <summary>Tears the whole field out of the tree.</summary>
    public void Free()
    {
        viewport.QueueFree();
        display.QueueFree();
    }

    /// <summary>
    /// Re-sizes the lane's canvas and its render resolution:
    /// <paramref name="pixelScale"/> is the window's canvas-to-pixel factor,
    /// so the lane renders at native resolution however the window is scaled.
    /// </summary>
    public void SetCanvas(Vector2 canvas, float pixelScale)
    {
        var size = new Vector2I(
            (int)Math.Max(Mathf.Round(canvas.X * pixelScale), 1.0f),
            (int)Math.Max(Mathf.Round(canvas.Y * pixelScale), 1.0f));
        if (this.canvas != canvas || viewport.Size != size)
        {
            this.canvas = canvas;
            viewport.Size = size;
            display.Texture = viewport.GetTexture();
        }
    }

    public void SetLayout(FieldLayout layout) => Layout = layout;

    private void SpawnReceptors()
    {
        for (uint column = 0; column < Layout.Columns; column++)
        {
            var visual = Skin.ReceptorVisual();
            var node = SpawnElement(visual, column, NoteField.TargetY, 10.0f, ColumnRotation(column));
            receptors.Add(new ReceptorEl(node, column, visual.Cell));
        }
    }

    public NoteIndex SpawnNote(NoteSpawn note)
    {
        var skinRow = Skin.Note.QuantRow(note.Quant);
        var rotation = ColumnRotation(note.Column);
        var nudge = BeatZNudge(note.Beat);

        ElementVisual visual;
        HoldEl? hold = null;
        if (note.Tail is { } tail)
        {
            var art = tail.Roll ? Skin.Roll : Skin.Hold;
            var bodyMaterial = SkinMaterials.Tail(art.BodyInactive);
            var capMaterial = SkinMaterials.Tail(art.CapInactive);
            var body = new MeshInstance3D { Mesh = Skin.QuadVisual(bodyMaterial).Mesh, Position = new Vector3(0.0f, OffscreenY, 18.0f - nudge), Visible = false };
            body.SetSurfaceOverrideMaterial(0, bodyMaterial);
            var cap = new MeshInstance3D { Mesh = Skin.QuadVisual(capMaterial).Mesh, Position = new Vector3(0.0f, OffscreenY, 18.2f - nudge), Visible = false };
            cap.SetSurfaceOverrideMaterial(0, capMaterial);
            space.AddChild(body);
            space.AddChild(cap);
            visual = Skin.HeadVisual(skinRow, false);
            hold = new HoldEl(tail.Time, tail.Beat, tail.Roll, body, bodyMaterial, cap, capMaterial);
        }
        else
        {
            visual = Skin.TapVisual(skinRow);
        }

        var noteNode = SpawnElement(visual, note.Column, OffscreenY, 20.0f - nudge, rotation);
        notes.Add(new NoteEl(noteNode, note.Time, note.Beat, note.Column, visual.Cell, skinRow, hold));
        return new NoteIndex(notes.Count - 1);
    }

    public MineIndex SpawnMine(Seconds time, Beat beat, uint column)
    {
        var visual = Skin.MineVisual();
        var node = SpawnElement(visual, column, OffscreenY, 20.0f - BeatZNudge(beat), Quaternion.Identity);
        mines.Add(new MineEl(node, time, beat, column, visual.Cell));
        return new MineIndex(mines.Count - 1);
    }

    private MeshInstance3D SpawnElement(ElementVisual visual, uint column, float y, float z, Quaternion rotation)
    {
        var node = new MeshInstance3D { Mesh = visual.Mesh };
        node.SetSurfaceOverrideMaterial(0, visual.Material);
        if (visual.Shell is { } shell)
        {
            node.SetSurfaceOverrideMaterial(1, shell);
        }

        var scale = Layout.ArrowSize / visual.Cell;
        node.Transform = new Transform3D(
            new Basis(rotation).Scaled(Vector3.One * scale),
            new Vector3(Layout.ColumnX(column), y, z));
        space.AddChild(node);
        return node;
    }

    /// <summary>Whether the panel of <paramref name="column"/> renders pressed.</summary>
    public void SetReceptorHeld(uint column, bool held)
    {
        foreach (var receptor in receptors)
        {
            if (receptor.Column == column)
            {
                receptor.Held = held;
            }
        }
    }

    public HoldVisualState? HoldState(NoteIndex note) => notes[note.Value].Hold?.State;

    public void SetHoldState(NoteIndex note, HoldVisualState state)
    {
        if (notes[note.Value].Hold is { } hold)
        {
            hold.State = state;
        }
    }

    /// <summary>Fades the note's head out where it stands (the hold-OK fade).</summary>
    public void FadeOutNote(NoteIndex note, float seconds)
    {
        var el = notes[note.Value];
        if (!el.Live)
        {
            return;
        }

        el.Live = false;
        fades.Add(new FadingElement(el.Node, null, seconds, 0.0f, el.Node.Scale, Colors.White));
        FreeHold(el);
    }

    /// <summary>Despawns the note on the spot, as grading does for vanished taps.</summary>
    public void VanishNote(NoteIndex note)
    {
        var el = notes[note.Value];
        if (!el.Live)
        {
            return;
        }

        el.Live = false;
        el.Node.QueueFree();
        FreeHold(el);
    }

    public void RemoveMine(MineIndex mine)
    {
        var el = mines[mine.Value];
        if (el.Live)
        {
            el.Live = false;
            el.Node.QueueFree();
        }
    }

    /// <summary>
    /// The arrow flash at a receptor when a step's arrows vanish, growing
    /// while it fades. The bright variant plays at high combo: larger art,
    /// snappier, starting smaller.
    /// </summary>
    public void ArrowFlash(uint column, float targetY, Color color, bool bright)
    {
        var (flash, seconds, baseZoom, growth) = bright
            ? (Skin.FlashBright, 0.13f, 0.8f, 0.5f)
            : (Skin.FlashDim, 0.18f, 1.0f, 0.4f);
        var size = flash.Size * (Layout.ArrowSize / NoteSkin.NoteCell) * baseZoom;
        Effect(
            SkinMaterials.Effect(flash.Texture, color),
            new Transform3D(
                new Basis(ColumnRotation(column)).Scaled(new Vector3(size.X, size.Y, 1.0f)),
                new Vector3(Layout.ColumnX(column), targetY, 22.0f)),
            seconds, growth);
    }

    public void MineExplosion(uint column, float targetY)
    {
        var scale = Layout.ArrowSize * 1.7f;
        Effect(
            SkinMaterials.Effect(Skin.MineExplosion.Texture, Colors.White),
            new Transform3D(Basis.FromScale(Vector3.One * scale), new Vector3(Layout.ColumnX(column), targetY, 21.0f)),
            MineExplosionSeconds, 0.25f);
    }

    private void Effect(StandardMaterial3D material, Transform3D transform, float seconds, float growth)
    {
        var node = new MeshInstance3D { Mesh = Skin.QuadVisual(material).Mesh, Transform = transform };
        node.SetSurfaceOverrideMaterial(0, material);
        space.AddChild(node);
        fades.Add(new FadingElement(node, material, seconds, growth, transform.Basis.Scale, material.AlbedoColor));
    }

    /// <summary>The whole field shuts down: notes, mines, and receptors shrink and fade away.</summary>
    public void FailOut(float seconds)
    {
        var doomed = new List<MeshInstance3D>();
        foreach (var note in notes)
        {
            if (note.Live)
            {
                note.Live = false;
                doomed.Add(note.Node);
                if (note.Hold is { } hold)
                {
                    doomed.Add(hold.Body);
                    doomed.Add(hold.Cap);
                    note.Hold = null;
                }
            }
        }

        foreach (var mine in mines)
        {
            if (mine.Live)
            {
                mine.Live = false;
                doomed.Add(mine.Node);
            }
        }

        foreach (var receptor in receptors)
        {
            doomed.Add(receptor.Node);
        }

        receptors.Clear();
        foreach (var node in doomed)
        {
            fades.Add(new FadingElement(node, null, seconds, -1.0f, node.Scale, Colors.White));
        }
    }

    /// <summary>One animation frame: scroll, textures, receptor press, hold parts, mines, transient fades, and the camera.</summary>
    public void Update(FieldClock clock, float delta)
    {
        var scroll = clock.Scroll();
        var beat = clock.Beat;

        elapsed += delta;
        AnimateSheetTaps(beat);
        AnimateReceptorFrames(beat);
        PlaceReceptors(clock.TargetY);
        AnimateReceptorPress(delta);
        ScrollAndAnimateNotes(scroll);
        AnimateMines(scroll, beat);
        ScrollModelTextures();
        RunFades(delta);
        SyncCamera(clock.TargetY);
    }

    private void ScrollAndAnimateNotes(NoteScroll scroll)
    {
        foreach (var el in notes)
        {
            if (!el.Live)
            {
                continue;
            }

            var pinned = el.Hold?.State.Pinned() ?? false;
            var y = scroll.YAt(Layout, el.Time, el.Beat);
            if (pinned)
            {
                y = Math.Min(y, scroll.TargetY);
            }

            var scale = Layout.ArrowSize / el.Cell;
            el.Node.Position = new Vector3(Layout.ColumnX(el.Column), y, el.Node.Position.Z);
            var wanted = Vector3.One * scale;
            if (el.Node.Scale != wanted)
            {
                el.Node.Scale = wanted;
            }

            if (el.Hold is not { } hold)
            {
                continue;
            }

            el.Node.SetSurfaceOverrideMaterial(0, Skin.HeadVisual(el.SkinRow, hold.State.Active()).Material);
            AnimateHoldParts(Layout, Skin, el.Column, y, scroll, hold);
        }
    }

    private void AnimateSheetTaps(Beat beat)
    {
        if (Skin.Note is not NoteArt.Sheet sheet)
        {
            return;
        }

        var x = sheet.Notes.FrameXAt(beat);
        foreach (var material in sheet.Notes.TapMaterials())
        {
            var offset = material.Uv1Offset;
            if (!Mathf.IsEqualApprox(offset.X, x))
            {
                material.Uv1Offset = new Vector3(x, offset.Y, offset.Z);
            }
        }
    }

    private void AnimateReceptorFrames(Beat beat)
    {
        var receptor = Skin.Receptor;
        var x = receptor.FrameXAt(beat);
        var brightness = receptor.BrightnessAt(beat);
        var color = new Color(brightness, brightness, brightness);
        var material = receptor.Material;
        var offset = material.Uv1Offset;
        if (!Mathf.IsEqualApprox(offset.X, x) || material.AlbedoColor != color)
        {
            material.Uv1Offset = new Vector3(x, offset.Y, offset.Z);
            material.AlbedoColor = color;
        }
    }

    /// <summary>Receptors follow the live target row, column, and arrow size every frame.</summary>
    private void PlaceReceptors(float targetY)
    {
        foreach (var receptor in receptors)
        {
            receptor.Node.Position = new Vector3(Layout.ColumnX(receptor.Column), targetY, receptor.Node.Position.Z);
            if (!receptor.Held && receptor.Press == 0.0f)
            {
                var wanted = Vector3.One * (Layout.ArrowSize / receptor.Cell);
                if (receptor.Node.Scale != wanted)
                {
                    receptor.Node.Scale = wanted;
                }
            }
        }
    }

    /// <summary>Held receptors tween back along Z with a shrink to sell the depth.</summary>
    private void AnimateReceptorPress(float delta)
    {
        foreach (var receptor in receptors)
        {
            if (!receptor.Held && receptor.Press == 0.0f)
            {
                continue;
            }

            var step = delta / PressSeconds;
            receptor.Press = Math.Clamp(receptor.Press + (receptor.Held ? step : -step), 0.0f, 1.0f);
            var eased = EaseCubicInOut(receptor.Press);
            var baseScale = Layout.ArrowSize / receptor.Cell;
            receptor.Node.Position = new Vector3(receptor.Node.Position.X, receptor.Node.Position.Y, 10.0f - (6.0f * eased));
            receptor.Node.Scale = Vector3.One * (baseScale * (1.0f - (0.22f * eased)));
        }
    }

    private void AnimateMines(NoteScroll scroll, Beat beat)
    {
        var spin = Skin.MineSpinBeats;
        foreach (var mine in mines)
        {
            if (!mine.Live)
            {
                continue;
            }

            var y = scroll.YAt(Layout, mine.Time, mine.Beat);
            var angle = (float)(-(SheetNotes.RemEuclid(beat.Value, spin) / spin) * Math.Tau);
            var scale = Layout.ArrowSize / mine.Cell;
            mine.Node.Position = new Vector3(Layout.ColumnX(mine.Column), y, mine.Node.Position.Z);
            mine.Node.Basis = new Basis(Vector3.Back, angle).Scaled(Vector3.One * scale);
        }
    }

    /// <summary>Drifts the texture coordinates of every scrolling model material.</summary>
    private void ScrollModelTextures()
    {
        foreach (var (material, baseOffset, velocity) in Skin.ScrollingMaterials())
        {
            if (velocity == Vector2.Zero)
            {
                continue;
            }

            material.Uv1Offset = new Vector3(
                (float)SheetNotes.RemEuclid(baseOffset.X + (velocity.X * elapsed), 1.0),
                (float)SheetNotes.RemEuclid(baseOffset.Y + (velocity.Y * elapsed), 1.0),
                0.0f);
        }
    }

    private void RunFades(float delta)
    {
        for (var i = fades.Count - 1; i >= 0; i--)
        {
            var fade = fades[i];
            fade.Remaining -= delta;
            if (fade.Remaining <= 0.0f)
            {
                fade.Node.QueueFree();
                fades.RemoveAt(i);
                continue;
            }

            var alpha = fade.Remaining / fade.Total;
            if (fade.Growth != 0.0f)
            {
                fade.Node.Scale = fade.BaseScale * (1.0f + (fade.Growth * (1.0f - alpha)));
            }

            if (fade.Material is { } material)
            {
                var color = fade.BaseColor;
                color.A *= alpha;
                material.AlbedoColor = color;
            }
        }
    }

    /// <summary>Keeps the lane camera over the field's center, pitched around the receptor row per the player's perspective.</summary>
    private void SyncCamera(float targetY)
    {
        var distance = canvas.Y * 0.5f / Mathf.Tan(fov * 0.5f);
        var near = distance * 0.05f;
        var far = distance * 4.0f;
        var pitch = perspective switch
        {
            Perspective.Above => -tilt,
            Perspective.Below => tilt,
            _ => 0.0f,
        };
        var originX = Layout.OriginX;
        var pivot = new Vector3(originX, targetY, 0.0f);
        var rotation = new Basis(Vector3.Right, pitch);
        var position = pivot + (rotation * (new Vector3(originX, 0.0f, distance) - pivot));
        camera.Transform = new Transform3D(rotation, position);
        camera.Near = near;
        camera.Far = far;
        camera.Size = 2.0f * near * Mathf.Tan(fov * 0.5f);
        camera.FrustumOffset = new Vector2(-originX * near / distance, 0.0f);
    }

    private static void FreeHold(NoteEl el)
    {
        if (el.Hold is { } hold)
        {
            hold.Body.QueueFree();
            hold.Cap.QueueFree();
            el.Hold = null;
        }
    }

    /// <summary>
    /// Each note sits slightly deeper the later its beat, so an earlier note
    /// always draws over ones scrolling in behind it under a flat camera. The
    /// nudge wraps far beyond any on-screen beat span.
    /// </summary>
    private static float BeatZNudge(Beat beat) => (float)(SheetNotes.RemEuclid(beat.Value, 256.0) * 0.005);

    /// <summary>
    /// The skin's arrows point down; rotate per pad-local column so every
    /// group of four reads Left, Down, Up, Right.
    /// </summary>
    public static Quaternion ColumnRotation(uint column)
    {
        var angle = (column % NoteField.PadColumns) switch
        {
            0 => -Mathf.Pi / 2.0f,
            1 => 0.0f,
            2 => Mathf.Pi,
            _ => Mathf.Pi / 2.0f,
        };
        return new Quaternion(Vector3.Back, angle);
    }

    private static float EaseCubicInOut(float t) =>
        t < 0.5f ? 4.0f * t * t * t : 1.0f - (Mathf.Pow((-2.0f * t) + 2.0f, 3) / 2.0f);

    /// <summary>
    /// Positions and styles the hold tail: the body is one quad whose texture
    /// wraps vertically, anchored to the tail so the pattern always meets the
    /// cap at a tile boundary, and the cap sits centered on the tail below it.
    /// </summary>
    private static void AnimateHoldParts(FieldLayout layout, NoteSkin skin, uint column, float headY, NoteScroll scroll, HoldEl hold)
    {
        var art = hold.Roll ? skin.Roll : skin.Hold;
        var scale = layout.ArrowSize / art.BodySize.X;
        var capHeight = art.CapSize.Y * scale;
        if (hold.State == HoldVisualState.Ok)
        {
            hold.Body.Visible = false;
            hold.Cap.Visible = false;
            return;
        }

        var endY = scroll.YAt(layout, hold.End, hold.EndBeat);
        var bodyBottom = endY + (art.StopAboveTail * scale);
        var x = layout.ColumnX(column);

        var active = hold.State.Active();
        var brightness = hold.State == HoldVisualState.Dropped ? skin.DroppedBrightness : 1.0f;
        var color = new Color(brightness, brightness, brightness);

        var length = headY - bodyBottom;
        if (length <= 0.5f)
        {
            hold.Body.Visible = false;
        }
        else
        {
            var height = length + BodyCapOverlap;
            var bodyZ = hold.Body.Position.Z;
            var top = art.BodySize.Y - (length / scale);
            var bottom = art.BodySize.Y + (BodyCapOverlap / scale);
            ApplyPart(hold.Body, hold.BodyMaterial, active ? art.BodyActive : art.BodyInactive,
                UvWindow(top, bottom, art.BodySize.Y), color,
                new Vector3(x, headY - (height / 2.0f), bodyZ), new Vector3(layout.ArrowSize, height, 1.0f));
        }

        var capTop = Math.Min(bodyBottom, headY);
        var capBottom = bodyBottom - capHeight;
        var visible = Math.Min(capTop - capBottom, capHeight);
        if (visible <= 0.5f)
        {
            hold.Cap.Visible = false;
        }
        else
        {
            var hidden = Math.Max(art.CapSize.Y - (visible / scale), 0.0f);
            var capZ = hold.Cap.Position.Z;
            ApplyPart(hold.Cap, hold.CapMaterial, active ? art.CapActive : art.CapInactive,
                UvWindow(hidden, art.CapSize.Y, art.CapSize.Y), color,
                new Vector3(x, capBottom + (visible / 2.0f), capZ), new Vector3(layout.ArrowSize, visible, 1.0f));
        }
    }

    /// <summary>The vertical texture window <c>top..bottom</c> as <c>(uv scale, uv offset)</c> for a unit quad.</summary>
    private static (Vector2 Scale, Vector2 Offset) UvWindow(float top, float bottom, float textureHeight) =>
        (new Vector2(1.0f, (bottom - top) / textureHeight), new Vector2(0.0f, top / textureHeight));

    private static void ApplyPart(MeshInstance3D node, StandardMaterial3D material, Texture2D texture,
        (Vector2 Scale, Vector2 Offset) uv, Color color, Vector3 position, Vector3 scale)
    {
        material.AlbedoTexture = texture;
        material.Uv1Scale = new Vector3(uv.Scale.X, uv.Scale.Y, 1.0f);
        material.Uv1Offset = new Vector3(uv.Offset.X, uv.Offset.Y, 0.0f);
        material.AlbedoColor = color;
        node.Position = position;
        node.Scale = scale;
        node.Visible = true;
    }

    private sealed class ReceptorEl(MeshInstance3D node, uint column, float cell)
    {
        public MeshInstance3D Node { get; } = node;
        public uint Column { get; } = column;
        public float Cell { get; } = cell;
        public bool Held { get; set; }
        public float Press { get; set; }
    }

    private sealed class NoteEl(MeshInstance3D node, Seconds time, Beat beat, uint column, float cell, int skinRow, HoldEl? hold)
    {
        public MeshInstance3D Node { get; } = node;
        public Seconds Time { get; } = time;
        public Beat Beat { get; } = beat;
        public uint Column { get; } = column;
        public float Cell { get; } = cell;
        public int SkinRow { get; } = skinRow;
        public HoldEl? Hold { get; set; } = hold;
        public bool Live { get; set; } = true;
    }

    private sealed class HoldEl(Seconds end, Beat endBeat, bool roll, MeshInstance3D body, StandardMaterial3D bodyMaterial, MeshInstance3D cap, StandardMaterial3D capMaterial)
    {
        public Seconds End { get; } = end;
        public Beat EndBeat { get; } = endBeat;
        public bool Roll { get; } = roll;
        public HoldVisualState State { get; set; } = HoldVisualState.Pending;
        public MeshInstance3D Body { get; } = body;
        public StandardMaterial3D BodyMaterial { get; } = bodyMaterial;
        public MeshInstance3D Cap { get; } = cap;
        public StandardMaterial3D CapMaterial { get; } = capMaterial;
    }

    private sealed class MineEl(MeshInstance3D node, Seconds time, Beat beat, uint column, float cell)
    {
        public MeshInstance3D Node { get; } = node;
        public Seconds Time { get; } = time;
        public Beat Beat { get; } = beat;
        public uint Column { get; } = column;
        public float Cell { get; } = cell;
        public bool Live { get; set; } = true;
    }

    private sealed class FadingElement(MeshInstance3D node, StandardMaterial3D? material, float total, float growth, Vector3 baseScale, Color baseColor)
    {
        public MeshInstance3D Node { get; } = node;
        public StandardMaterial3D? Material { get; } = material;
        public float Remaining { get; set; } = total;
        public float Total { get; } = total;
        public float Growth { get; } = growth;
        public Vector3 BaseScale { get; } = baseScale;
        public Color BaseColor { get; } = baseColor;
    }
}
