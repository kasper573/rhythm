using Godot;
using Rhythm.Core;

namespace Rhythm;

/// <summary>
/// One player's high-score rating image at the right edge of a stepfile row;
/// versus shows both players' inline, with a P1/P2 tag pinned to the image's
/// bottom-left corner once its width is known.
/// </summary>
public sealed class RatingUi
{
    /// <summary>On-screen height of the wheel's rating images (native art is 120px).</summary>
    public const float RatingHeight = 38.0f;

    /// <summary>
    /// Right edge of the rating column in row-local coordinates; the ratings
    /// are children of their wheel row, so they ride it at this inset from its
    /// center as it scrolls and bulges.
    /// </summary>
    public const float RatingRightX = 270.0f;

    /// <summary>
    /// Width reserved per player's rating in versus, where both are inline:
    /// wide enough for the widest rating art at <see cref="RatingHeight"/>.
    /// </summary>
    public const float RatingSlotWidth = 92.0f;

    public PlayerId Player { get; }
    public Sprite2D Sprite { get; }
    public Label Tag { get; }

    /// <summary>The loaded art's native width, for the right-edge anchor and the tag; null until it arrives.</summary>
    public float? Width { get; set; }
    public string? Source { get; set; }
    public PendingTexture? Pending { get; set; }

    private RatingUi(PlayerId player, Sprite2D sprite, Label tag)
    {
        Player = player;
        Sprite = sprite;
        Tag = tag;
    }

    public static RatingUi Spawn(Node2D row, PlayerId player)
    {
        var sprite = new Sprite2D
        {
            Centered = false,
            Scale = Vector2.One * (RatingHeight / 120.0f),
            Position = new Vector2(RatingRightX, 0.0f),
            Visible = false,
            ZIndex = 1,
        };
        var tag = Text.Label(player == PlayerId.P1 ? "P1" : "P2", 34.0f, new Color(1, 1, 1, 0.9f));
        sprite.AddChild(tag);
        tag.Position = new Vector2(4.0f, 120.0f - 34.0f);
        row.AddChild(sprite);
        return new RatingUi(player, sprite, tag);
    }
}

public partial class Wheel
{
    /// <summary>
    /// Packs each player's rating into its row's rating column: in versus both
    /// players' ratings sit inline, the last active player's at the right edge
    /// and earlier players stacked to its left.
    /// </summary>
    private void PackPlayerRatings()
    {
        foreach (var slot in slots)
        {
            foreach (var player in System.Enum.GetValues<PlayerId>())
            {
                var rating = slot.Ratings[player];
                int position = players.IndexOf(rating.Player);
                if (position < 0)
                {
                    position = 0;
                }
                float packed = Math.Max(0, players.Count - 1 - position) * RatingUi.RatingSlotWidth;
                float rightX = RatingUi.RatingRightX - packed;
                // Anchored by the image's right edge and vertical center.
                float width = rating.Width ?? 120.0f;
                float scale = RatingUi.RatingHeight / 120.0f;
                rating.Sprite.Position = new Vector2(rightX - width * scale, -RatingUi.RatingHeight / 2.0f);
            }
        }
    }

    /// <summary>
    /// Pins each rating's P1/P2 tag to the image's bottom-left corner and
    /// resolves textures that finished loading; the rating art varies in width,
    /// so both wait for the image.
    /// </summary>
    private void PositionRatingLabels()
    {
        foreach (var slot in slots)
        {
            foreach (var player in System.Enum.GetValues<PlayerId>())
            {
                var rating = slot.Ratings[player];
                if (rating.Pending is PendingTexture pending && pending.Poll() is PendingTexture.Loaded loaded)
                {
                    rating.Pending = null;
                    if (loaded.Texture is Texture2D texture)
                    {
                        rating.Width = texture.GetWidth();
                        rating.Sprite.Texture = texture;
                        rating.Sprite.Visible = true;
                        rating.Tag.Position = new Vector2(4.0f, texture.GetHeight() - 40.0f);
                    }
                }
            }
        }
    }

    private void RefreshWheelRatings()
    {
        if (!dirty)
        {
            return;
        }
        for (int index = 0; index < slots.Count; index++)
        {
            var entry = EntryAt(index);
            foreach (var player in System.Enum.GetValues<PlayerId>())
            {
                var wanted = entry is WheelEntry.Stepfile row && players.Contains(player)
                    ? HighScoreRating(row.Id, player)
                    : null;
                var rating = slots[index].Ratings[player];
                if (wanted == rating.Source)
                {
                    if (wanted is null)
                    {
                        rating.Sprite.Visible = false;
                    }
                    continue;
                }
                rating.Source = wanted;
                if (wanted is not null)
                {
                    rating.Pending = PendingTexture.Load(wanted);
                }
                else
                {
                    rating.Pending = null;
                    rating.Sprite.Visible = false;
                }
            }
        }
    }

    /// <summary>
    /// The rating image earned by the player's high score on the chart this row
    /// would currently play them. Only the total points are stored, so
    /// grade-based rating rules never match here.
    /// </summary>
    private string? HighScoreRating(StepfileId id, PlayerId player)
    {
        var entry = Library.Instance.Stepfile(id);
        if (ChartFor(entry.Stepfile, player) is not int chartIndex)
        {
            return null;
        }
        var chart = entry.Stepfile.Charts[chartIndex];
        var key = HighScores.HighscoreKey(Library.Instance, id, chart);
        if (HighScores.Instance.Get(player, key) is not uint points)
        {
            return null;
        }
        var stats = chart.Stats();
        var percent = Config.Current!.ScorePercent(points, (uint)chart.Rows.Count, (uint)stats.Holds);
        return Assets.Path(Config.Current.Rating(percent, null).Image);
    }
}
