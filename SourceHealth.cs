using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;

namespace YTLiveStats
{
    /// <summary>
    /// Append-only log of what every data source did, written to
    /// <c>%APPDATA%\YTLiveStats\sources.log</c>.
    ///
    /// A line is only written when the outcome for one source changes, so the file reads as
    /// "html stopped parsing at 13:42, innertube-about took over" instead of one line per poll.
    /// Logging never throws: a failure here must not disturb the overlay.
    /// </summary>
    public static class SourceHealth
    {
        /// <summary>Rotate to <c>sources.log.1</c> once the log grows past this size.</summary>
        public const int MaxBytes = 256 * 1024;

        public const string Ok = "OK";
        public const string Empty = "LEEG";
        public const string Failed = "FOUT";

        private static readonly object Gate = new();
        private static readonly Dictionary<string, string> LastOutcome = new(StringComparer.Ordinal);

        public static string LogPath => Path.Combine(
            Path.GetDirectoryName(AppSettings.FilePath)!, "sources.log");

        /// <summary>
        /// Records what one source did. <paramref name="kind"/> is "kanaal" or "video",
        /// <paramref name="target"/> the handle or video id, <paramref name="source"/> the label used
        /// in the status line ("html", "innertube-about", "watch-html", …).
        /// </summary>
        public static void Record(string kind, string target, string source, string outcome, string detail = "")
        {
            string key = kind + "|" + target + "|" + source;
            lock (Gate)
            {
                if (LastOutcome.TryGetValue(key, out string? previous) && previous == outcome)
                {
                    return;
                }

                LastOutcome[key] = outcome;
                Write(Format(kind, target, source, outcome, detail));
            }
        }

        private static string Format(string kind, string target, string source, string outcome, string detail) =>
            string.Format(
                CultureInfo.InvariantCulture,
                "{0}  {1}  {2}  {3}  {4}{5}",
                DateTimeOffset.Now.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture),
                Pad(kind, 7),
                Pad(target, 20),
                Pad(source, 18),
                Pad(outcome, 5),
                detail);

        private static string Pad(string value, int width)
        {
            value ??= "";
            return value.Length >= width ? value[..width] : value.PadRight(width);
        }

        private static void Write(string line)
        {
            try
            {
                string path = LogPath;
                Directory.CreateDirectory(Path.GetDirectoryName(path)!);

                var info = new FileInfo(path);
                if (info.Exists && info.Length > MaxBytes)
                {
                    string previous = path + ".1";
                    if (File.Exists(previous))
                    {
                        File.Delete(previous);
                    }

                    File.Move(path, previous);
                    LastOutcome.Clear();
                }

                File.AppendAllText(path, line + Environment.NewLine, new UTF8Encoding(false));
            }
            catch
            {
                // the log is diagnostics only; never let it break a poll
            }
        }
    }
}
