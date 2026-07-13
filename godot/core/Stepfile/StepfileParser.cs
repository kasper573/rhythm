using System.Globalization;

namespace Rhythm.Core;

/// <summary>Parses StepMania <c>.sm</c> simfile text into a <see cref="Stepfile"/>.</summary>
public static class StepfileParser
{
    /// <summary>The standard note values, quarters through 64ths, as notes-per-measure.</summary>
    private static readonly uint[] QuantLadder = [4, 8, 12, 16, 24, 32, 48, 64];

    public static Stepfile Parse(string text)
    {
        text = StripComments(text);

        var title = string.Empty;
        var subtitle = string.Empty;
        var artist = string.Empty;
        var titleTranslit = string.Empty;
        var subtitleTranslit = string.Empty;
        var artistTranslit = string.Empty;
        var credit = string.Empty;
        string? banner = null;
        string? background = null;
        string? cdTitle = null;
        string? music = null;
        var offset = Seconds.Zero;
        var sampleStart = Seconds.Zero;
        var sampleLength = new Seconds(10.0);
        var selectable = true;
        DisplayBpm? displayBpm = null;
        var bpms = new List<(Beat, Bpm)>();
        var stops = new List<(Beat, Seconds)>();
        IReadOnlyList<BgChange> bgChanges = [];
        var charts = new List<Chart>();
        var extraTags = new SortedDictionary<string, string>(StringComparer.Ordinal);

        foreach (var (name, value) in ScanTags(text))
        {
            var trimmed = value.Trim();
            switch (name)
            {
                case "TITLE": title = trimmed; break;
                case "SUBTITLE": subtitle = trimmed; break;
                case "ARTIST": artist = trimmed; break;
                case "TITLETRANSLIT": titleTranslit = trimmed; break;
                case "SUBTITLETRANSLIT": subtitleTranslit = trimmed; break;
                case "ARTISTTRANSLIT": artistTranslit = trimmed; break;
                case "CREDIT": credit = trimmed; break;
                case "BANNER": banner = NonEmpty(trimmed); break;
                case "BACKGROUND": background = NonEmpty(trimmed); break;
                case "CDTITLE": cdTitle = NonEmpty(trimmed); break;
                case "MUSIC": music = NonEmpty(trimmed); break;
                case "OFFSET": offset = new Seconds(ParseNumber(trimmed)); break;
                case "SAMPLESTART": sampleStart = new Seconds(ParseNumber(trimmed)); break;
                case "SAMPLELENGTH": sampleLength = new Seconds(ParseNumber(trimmed)); break;
                case "SELECTABLE": selectable = !trimmed.Equals("no", StringComparison.OrdinalIgnoreCase); break;
                case "DISPLAYBPM": displayBpm = ParseDisplayBpm(trimmed); break;
                case "BPMS":
                    bpms = ParseBeatNumberPairs(trimmed).Select(pair => (pair.Beat, new Bpm(pair.Number))).ToList();
                    break;

                // Delays differ from stops only for notes exactly on the pause
                // beat, which classic .sm files don't rely on.
                case "STOPS" or "DELAYS" or "FREEZES":
                    stops.AddRange(ParseBeatNumberPairs(trimmed).Select(pair => (pair.Beat, new Seconds(pair.Number))));
                    break;

                case "BGCHANGES" or "ANIMATIONS":
                    bgChanges = ParseBgChanges(trimmed);
                    break;

                case "NOTES":
                    if (ParseChart(value) is { } chart)
                    {
                        charts.Add(chart);
                    }

                    break;

                default:
                    extraTags[name] = trimmed;
                    break;
            }
        }

        if (!bpms.Any(pair => pair.Item2.Value > 0.0))
        {
            throw new StepfileException("stepfile has no valid #BPMS");
        }

        return new Stepfile
        {
            Title = title,
            Subtitle = subtitle,
            Artist = artist,
            TitleTranslit = titleTranslit,
            SubtitleTranslit = subtitleTranslit,
            ArtistTranslit = artistTranslit,
            Credit = credit,
            Banner = banner,
            Background = background,
            CdTitle = cdTitle,
            Music = music,
            SampleStart = sampleStart,
            SampleLength = sampleLength,
            Selectable = selectable,
            DisplayBpm = displayBpm,
            Timing = new StepfileTiming(offset, bpms, stops),
            BgChanges = bgChanges,
            Charts = charts,
            ExtraTags = extraTags,
        };
    }

    /// <summary>Returns <c>(NAME, value)</c> for each <c>#NAME:value;</c> tag, with names upper-cased.</summary>
    private static List<(string Name, string Value)> ScanTags(string text)
    {
        var tags = new List<(string, string)>();
        var cursor = 0;
        while (true)
        {
            var hash = text.IndexOf('#', cursor);
            if (hash < 0)
            {
                return tags;
            }

            var colon = text.IndexOf(':', hash + 1);
            if (colon < 0)
            {
                return tags;
            }

            var name = text[(hash + 1)..colon].Trim().ToUpperInvariant();
            var valueStart = colon + 1;
            var end = text.IndexOf(';', valueStart);
            if (end < 0)
            {
                end = text.Length;
            }

            tags.Add((name, text[valueStart..end]));
            cursor = Math.Min(end + 1, text.Length);
        }
    }

    private static string StripComments(string text) =>
        string.Join('\n', text.Split('\n').Select(line =>
        {
            var comment = line.IndexOf("//", StringComparison.Ordinal);
            return comment < 0 ? line : line[..comment];
        }));

    private static string? NonEmpty(string value) => value.Length == 0 ? null : value;

    private static double ParseNumber(string value) =>
        double.TryParse(value.Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out var number) ? number : 0.0;

    private static List<(Beat Beat, double Number)> ParseBeatNumberPairs(string value)
    {
        var pairs = new List<(Beat, double)>();
        foreach (var entry in value.Split(','))
        {
            var equals = entry.IndexOf('=');
            if (equals < 0)
            {
                continue;
            }

            if (double.TryParse(entry[..equals].Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out var beat) &&
                double.TryParse(entry[(equals + 1)..].Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out var number))
            {
                pairs.Add((new Beat(beat), number));
            }
        }

        return pairs;
    }

    private static DisplayBpm? ParseDisplayBpm(string value)
    {
        if (value.Length == 0)
        {
            return null;
        }

        if (value.StartsWith('*'))
        {
            return new DisplayBpm.Random();
        }

        var colon = value.IndexOf(':');
        if (colon >= 0)
        {
            if (TryBpm(value[..colon], out var low) && TryBpm(value[(colon + 1)..], out var high))
            {
                return new DisplayBpm.Range(low, high);
            }

            return null;
        }

        return TryBpm(value, out var single) ? new DisplayBpm.Single(single) : null;

        static bool TryBpm(string text, out Bpm bpm)
        {
            if (double.TryParse(text.Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out var number))
            {
                bpm = new Bpm(number);
                return true;
            }

            bpm = default;
            return false;
        }
    }

    /// <summary>
    /// Fields per entry: <c>beat=file=rate=crossfade=rewind=loop=effect=...</c>.
    /// The effect resolves as: looping by default, the no-loop flag beats the
    /// rewind flag, and an explicit effect name beats both (rewind
    /// approximates to looping — the movie keeps moving).
    /// </summary>
    private static List<BgChange> ParseBgChanges(string value)
    {
        var changes = new List<BgChange>();
        foreach (var entry in value.Split(','))
        {
            var fields = entry.Trim().Split('=').Select(field => field.Trim()).ToArray();
            if (fields.Length < 2 ||
                !double.TryParse(fields[0], NumberStyles.Float, CultureInfo.InvariantCulture, out var beat))
            {
                continue;
            }

            var file = fields[1];
            if (file.Length == 0)
            {
                continue;
            }

            var loops = fields.Length <= 5 || fields[5] != "0";
            if (fields.Length > 6)
            {
                var effect = fields[6];
                if (effect == "StretchNoLoop")
                {
                    loops = false;
                }
                else if (effect.Length > 0)
                {
                    loops = true;
                }
            }

            changes.Add(new BgChange(new Beat(beat), file, Flag(fields, 3), loops));
        }

        return changes;

        static bool Flag(string[] fields, int index) =>
            index < fields.Length && int.TryParse(fields[index], out var flag) && flag != 0;
    }

    private static Chart? ParseChart(string value)
    {
        var parts = value.Split(':', 6);
        if (parts.Length != 6)
        {
            return null;
        }

        var stepsType = StepsType.Parse(parts[0].Trim());
        var noteData = ParseNoteData(parts[5], stepsType.Columns());
        if (noteData is not var (columns, rows, mines))
        {
            return null;
        }

        return new Chart
        {
            StepsType = stepsType,
            Description = parts[1].Trim(),
            Difficulty = Difficulty.Parse(parts[2].Trim()),
            Meter = uint.TryParse(parts[3].Trim(), out var meter) ? meter : 0,
            Radar = parts[4]
                .Split(',')
                .Select(radar => float.TryParse(radar.Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out var value) ? (float?)value : null)
                .Where(value => value is not null)
                .Select(value => value!.Value)
                .ToList(),
            Columns = columns,
            Rows = rows,
            Mines = mines,
        };
    }

    /// <summary>
    /// Parses measure-based note data. Returns the column count (inferred
    /// from the first row when the steps type doesn't dictate one), the
    /// steppable rows, and the mines. Lifts and fakes are consumed but
    /// produce nothing.
    /// </summary>
    private static (int Columns, List<Row> Rows, List<Mine> Mines)? ParseNoteData(string data, int? knownColumns)
    {
        var measures = data
            .Split(',')
            .Select(measure => measure
                .Split('\n')
                .Select(line => line.Trim())
                .Where(line => line.Length > 0 && line.All(IsNoteChar))
                .ToList())
            .ToList();

        var columns = knownColumns ?? measures.SelectMany(measure => measure).FirstOrDefault()?.Length;
        if (columns is not { } columnCount)
        {
            return null;
        }

        // (beat, quant, column, tail) per steppable arrow; grouped into rows
        // after the hold tails have resolved.
        var arrows = new List<(Beat Beat, uint Quant, int Column, Tail? Tail)>();
        var mines = new List<Mine>();
        var openHolds = new (Beat Beat, uint Quant, bool Roll)?[columnCount];

        for (var measureIndex = 0; measureIndex < measures.Count; measureIndex++)
        {
            var lines = measures[measureIndex];
            var lineCount = lines.Count;
            for (var lineIndex = 0; lineIndex < lineCount; lineIndex++)
            {
                var line = lines[lineIndex];
                var beat = new Beat((measureIndex * 4.0) + (lineIndex * 4.0 / lineCount));
                var quant = Quantization(lineIndex, lineCount);
                for (var column = 0; column < columnCount && column < line.Length; column++)
                {
                    switch (line[column])
                    {
                        case '1':
                            arrows.Add((beat, quant, column, null));
                            break;

                        // A head overwritten before its tail is orphaned like
                        // an unclosed one: it still demands a step.
                        case '2' or '4':
                            if (openHolds[column] is { } orphan)
                            {
                                arrows.Add((orphan.Beat, orphan.Quant, column, null));
                            }

                            openHolds[column] = (beat, quant, line[column] == '4');
                            break;

                        case '3':
                            if (openHolds[column] is { } head)
                            {
                                arrows.Add((head.Beat, head.Quant, column, new Tail(beat, head.Roll)));
                                openHolds[column] = null;
                            }

                            break;

                        case 'M' or 'm':
                            mines.Add(new Mine(beat, column));
                            break;
                    }
                }
            }
        }

        // A hold head whose tail never appears still demands a step.
        for (var column = 0; column < columnCount; column++)
        {
            if (openHolds[column] is { } open)
            {
                arrows.Add((open.Beat, open.Quant, column, null));
            }
        }

        arrows.Sort((a, b) =>
        {
            var byBeat = a.Beat.Value.CompareTo(b.Beat.Value);
            return byBeat != 0 ? byBeat : a.Column.CompareTo(b.Column);
        });

        var rows = new List<Row>();
        var pending = new List<(int, Arrow)>();
        foreach (var (beat, quant, column, tail) in arrows)
        {
            if (rows.Count > 0 && rows[^1].Beat == beat)
            {
                var last = rows[^1];
                ((List<Arrow>)last.Arrows).Add(new Arrow(column, tail));
            }
            else
            {
                rows.Add(new Row(beat, quant, new List<Arrow> { new(column, tail) }));
            }
        }

        _ = pending;
        mines.Sort((a, b) =>
        {
            var byBeat = a.Beat.Value.CompareTo(b.Beat.Value);
            return byBeat != 0 ? byBeat : a.Column.CompareTo(b.Column);
        });
        return (columnCount, rows, mines);
    }

    /// <summary>
    /// The note value of row <paramref name="rowIndex"/> in a measure of
    /// <paramref name="rowCount"/> rows: the coarsest standard grid the row
    /// lands on. The row's exact position is the reduced fraction
    /// <c>rowIndex / rowCount</c>; the note value is the first ladder entry
    /// that denominator divides (a row at 1/6 of a measure sits on the
    /// 12th-note grid). Positions off every standard grid keep their exact
    /// denominator.
    /// </summary>
    private static uint Quantization(int rowIndex, int rowCount)
    {
        var denominator = (uint)(rowCount / Gcd(rowIndex, rowCount));
        foreach (var quant in QuantLadder)
        {
            if (quant % denominator == 0)
            {
                return quant;
            }
        }

        return denominator;
    }

    private static int Gcd(int a, int b) => a == 0 ? b : Gcd(b % a, a);

    private static bool IsNoteChar(char c) =>
        c is (>= '0' and <= '9') or (>= 'A' and <= 'Z') or (>= 'a' and <= 'z');
}
