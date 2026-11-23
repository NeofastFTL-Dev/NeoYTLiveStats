using System;
using System.IO;
using System.Text.Json;

namespace YTLiveStats
{
    /// <summary>
    /// User-editable configuration, stored in %APPDATA%\YTLiveStats\settings.json.
    /// Command-line arguments override these values for the current session only.
    /// </summary>
    public sealed class AppSettings
    {
        public string Handle { get; set; } = "NeofastFTL_YT";
        public string VideoId { get; set; } = "vOCHvWdeWTE";

        /// <summary>White flash every N new subscribers.</summary>
        public long SubsFlashStep { get; set; } = 10;

        /// <summary>Rainbow flash every N new video views.</summary>
        public long VideoViewsFlashStep { get; set; } = 100;

        /// <summary>Rainbow flash every N new channel views.</summary>
        public long ChannelViewsFlashStep { get; set; } = 100;

        /// <summary>
        /// The app kept its files in %APPDATA%\NeoYTLiveStats before it was renamed. Move that folder
        /// across on the first start afterwards, so the settings, the source health log and the saved
        /// counters are still there.
        /// </summary>
        public static void MigrateLegacyFolder()
        {
            try
            {
                string appData = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
                string legacy = Path.Combine(appData, "NeoYTLiveStats");
                string current = Path.Combine(appData, "YTLiveStats");
                if (Directory.Exists(legacy) && !Directory.Exists(current))
                {
                    Directory.Move(legacy, current);
                }
            }
            catch
            {
                // Best effort: if the folder cannot be moved the app simply starts with defaults.
            }
        }

        public static string FilePath => Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "YTLiveStats", "settings.json");

        public static AppSettings Load()
        {
            try
            {
                if (File.Exists(FilePath))
                {
                    string json = File.ReadAllText(FilePath);
                    var loaded = JsonSerializer.Deserialize<AppSettings>(json);
                    if (loaded != null)
                    {
                        return loaded.Normalized();
                    }
                }
            }
            catch
            {
                // unreadable or corrupt file: fall back to defaults
            }
            return new AppSettings();
        }

        public void Save()
        {
            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);
                File.WriteAllText(FilePath, JsonSerializer.Serialize(this,
                    new JsonSerializerOptions { WriteIndented = true }));
            }
            catch
            {
                // saving is best effort; the running window keeps the new values
            }
        }

        public AppSettings Normalized()
        {
            Handle = string.IsNullOrWhiteSpace(Handle) ? "NeofastFTL_YT" : Handle.Trim().TrimStart('@');
            if (string.IsNullOrWhiteSpace(VideoId))
            {
                VideoId = "vOCHvWdeWTE";
            }
            VideoId = YtApi.ExtractVideoId(VideoId) ?? VideoId.Trim();

            if (SubsFlashStep <= 0)
            {
                SubsFlashStep = 10;
            }
            if (VideoViewsFlashStep <= 0)
            {
                VideoViewsFlashStep = 100;
            }
            if (ChannelViewsFlashStep <= 0)
            {
                ChannelViewsFlashStep = 100;
            }
            return this;
        }

        public AppSettings Clone() => (AppSettings)MemberwiseClone();
    }
}
