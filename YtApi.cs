using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Net;
using System.Net.Http;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;

namespace YTLiveStats
{
    /// <summary>
    /// Reads live numbers off YouTube.
    ///
    /// Every number has more than one source, so a markup or payload change turns into a fallback
    /// instead of a blank screen:
    ///   channel: about page HTML -> InnerTube browse (about params) -> InnerTube browse (plain) -> InnerTube MWEB
    ///   video:   InnerTube WEB player -> watch page HTML -> InnerTube ANDROID player
    ///
    /// When a source answers but its payload cannot be parsed, <c>FormatChanged</c> is set and the
    /// failure is listed in <c>Error</c>, which is what distinguishes "YouTube changed something" from
    /// "the network is down".
    /// </summary>
    public static class YtApi
    {
        public sealed class ChannelStats
        {
            public string Handle = "";
            public string Title = "";
            public string ChannelId = "";
            public long? Subscribers;
            public bool SubsApprox;
            public string SubsText = "";
            public long? Views;
            public long? Videos;
            public string Error = "";

            /// <summary>Source that produced the numbers ("html", "innertube-about", …), "" when none did.</summary>
            public string Source = "";

            /// <summary>True when a source responded but the payload had no numbers we recognise.</summary>
            public bool FormatChanged;
        }

        public sealed class VideoStats
        {
            public string VideoId = "";
            public string Title = "";
            public long? Views;
            public string Error = "";

            /// <summary>Source that produced the numbers ("player", "watch-html", "player-android"), "" when none did.</summary>
            public string Source = "";

            /// <summary>True when a source responded but the payload had no numbers we recognise.</summary>
            public bool FormatChanged;
        }

        private static readonly HttpClient Http = CreateClient();

        private const string InnertubeKey = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Wa8ESU4OY";
        private const string WebClientVer = "2.20261026.01.00";
        private const string AndroidClientVer = "19.09.37";
        private const string KnownMainChannelId = "UCYLki_57xkd9KoKygzB0q3A";
        private const string AboutParams = "EgVhYm91dA%3D%3D";

        private static readonly ConcurrentDictionary<string, string> _cachedChannelIds =
            new(StringComparer.OrdinalIgnoreCase);

        private static long Now() => DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();

        private static HttpClient CreateClient()
        {
            var handler = new HttpClientHandler
            {
                AutomaticDecompression = DecompressionMethods.All,
                UseCookies = true,
                CookieContainer = new CookieContainer()
            };
            var yt = new Uri("https://www.youtube.com/");
            handler.CookieContainer.Add(yt, new Cookie("CONSENT", "YES+cb.20210328-17-p0.en+FX+410"));
            handler.CookieContainer.Add(yt, new Cookie("SOCS",
                "CAISNQgDEitib3FfaWRlbnRpdHlmcm9udGVuZHVpc2VydmVyXzIwMjMwODI5LjA3X3AxGgJlbiACGgYIgLC_pwY"));

            var http = new HttpClient(handler);
            http.DefaultRequestHeaders.TryAddWithoutValidation("User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");
            http.DefaultRequestHeaders.TryAddWithoutValidation("Accept-Language", "en-US,en;q=0.9");
            http.DefaultRequestHeaders.TryAddWithoutValidation("Accept", "text/html,application/xhtml+xml,application/json");
            http.DefaultRequestHeaders.TryAddWithoutValidation("Cache-Control", "no-cache, no-store, must-revalidate");
            http.DefaultRequestHeaders.TryAddWithoutValidation("Pragma", "no-cache");
            http.Timeout = TimeSpan.FromSeconds(15);
            return http;
        }

        public static async Task<ChannelStats> FetchChannelAsync(string handle,
            CancellationToken ct = default)
        {
            handle = handle.TrimStart('@').Trim();
            var s = new ChannelStats { Handle = handle };
            var failures = new List<string>();

            // Source 1: the public channel about page.
            bool htmlAnswered = false;
            try
            {
                string url = $"https://www.youtube.com/@{Uri.EscapeDataString(handle)}/about?hl=en&gl=US&_t={Now()}";
                string html = await Http.GetStringAsync(url, ct);
                htmlAnswered = true;
                ParseAboutHtml(html, s);
                if (!string.IsNullOrEmpty(s.ChannelId))
                {
                    _cachedChannelIds[handle] = s.ChannelId;
                }

                if (HasNumbers(s))
                {
                    s.Source = "html";
                    SourceHealth.Record("kanaal", "@" + handle, "html", SourceHealth.Ok, Detail(s));
                }
                else if (string.Equals(s.Title, "Before you continue to YouTube", StringComparison.OrdinalIgnoreCase))
                {
                    failures.Add("html: toestemmingspagina");
                    SourceHealth.Record("kanaal", "@" + handle, "html", SourceHealth.Empty, "toestemmingspagina");
                }
                else
                {
                    s.FormatChanged = true;
                    failures.Add("html: cijfers niet in de pagina");
                    SourceHealth.Record("kanaal", "@" + handle, "html", SourceHealth.Empty,
                        "cijfers niet in de pagina");
                }
            }
            catch (Exception ex)
            {
                failures.Add("html: " + ex.Message);
                SourceHealth.Record("kanaal", "@" + handle, "html", SourceHealth.Failed, ex.Message);
            }

            // Source 2..4: InnerTube, with the channel id taken from the page, the cache or the resolve endpoint.
            if (!HasNumbers(s))
            {
                string? channelId = s.ChannelId;
                if (string.IsNullOrEmpty(channelId) && _cachedChannelIds.TryGetValue(handle, out string? cached))
                {
                    channelId = cached;
                }
                if (string.IsNullOrEmpty(channelId) && handle.Equals("NeofastFTL_YT", StringComparison.OrdinalIgnoreCase))
                {
                    channelId = KnownMainChannelId;
                }
                if (string.IsNullOrEmpty(channelId))
                {
                    channelId = await ResolveChannelIdAsync(handle, ct);
                }

                if (string.IsNullOrEmpty(channelId))
                {
                    failures.Add("kanaal-id niet gevonden");
                }
                else
                {
                    s.ChannelId = channelId;
                    _cachedChannelIds[handle] = channelId;
                    await TryInnertubeChannelAsync(s, channelId, failures, ct);
                }
            }

            bool lowViews = s.Views.HasValue && s.Views < 1000
                            && handle.Equals("NeofastFTL_YT", StringComparison.OrdinalIgnoreCase);

            if (HasNumbers(s))
            {
                if (lowViews)
                {
                    s.Error = "Views te laag (waarschijnlijk video i.p.v. kanaal): " + s.Views;
                }
                else if (!s.Subscribers.HasValue)
                {
                    s.Error = "Geen subscribers in deze bron (" +
                              (string.IsNullOrEmpty(s.Source) ? "?" : s.Source) + ")";
                }
                else if (!s.Views.HasValue)
                {
                    s.Error = "Geen kanaal views in deze bron (" +
                              (string.IsNullOrEmpty(s.Source) ? "?" : s.Source) + ")";
                }
                else
                {
                    s.Error = "";
                }
            }
            else
            {
                if (lowViews)
                {
                    failures.Add("views te laag: " + s.Views);
                }
                if (htmlAnswered)
                {
                    s.FormatChanged = true;
                }
                s.Error = failures.Count > 0 ? string.Join(" | ", failures) : "geen kanaalcijfers";
                if (s.FormatChanged && failures.Count > 0)
                {
                    s.Error += " (formaat gewijzigd?)";
                }
            }

            return s;
        }

        /// <summary>
        /// True when a source produced at least one of the numbers the overlay needs. InnerTube does
        /// not carry the channel-wide view total at all, so requiring both would make every fallback
        /// look broken; the missing piece is reported instead.
        /// </summary>
        private static bool HasNumbers(ChannelStats s) => s.Subscribers.HasValue || s.Views.HasValue;

        /// <summary>Short "what did this source actually give us" text for the health log.</summary>
        private static string Detail(ChannelStats s) =>
            "subs=" + (s.Subscribers.HasValue ? s.Subscribers.Value.ToString(CultureInfo.InvariantCulture) : "-") +
            " views=" + (s.Views.HasValue ? s.Views.Value.ToString(CultureInfo.InvariantCulture) : "-");

        private static void ParseAboutHtml(string html, ChannelStats s)
        {
            var m = Regex.Match(html, "\"externalId\":\"(UC[^\"]+)\"");
            if (!m.Success)
            {
                m = Regex.Match(html, "\"channelId\":\"(UC[^\"]+)\"");
            }
            if (m.Success)
            {
                s.ChannelId = m.Groups[1].Value;
            }

            m = Regex.Match(html, "\"channelMetadataRenderer\":\\{\"title\":\"([^\"]+)\"");
            if (m.Success)
            {
                s.Title = Unescape(m.Groups[1].Value);
            }
            if (string.IsNullOrEmpty(s.Title))
            {
                m = Regex.Match(html, "<title>([^<]+)</title>");
                if (m.Success)
                {
                    s.Title = m.Groups[1].Value.Replace(" - YouTube", "").Trim();
                }
            }

            TryParseSubscribers(html, s, requireOwnHeader: true);

            m = Regex.Match(html, "\"viewCountText\":\"([^\"]+)\"");
            if (!m.Success)
            {
                m = Regex.Match(html, "\"viewCountText\":\\{[^}]*?\"simpleText\":\"([^\"]+)\"");
            }
            if (m.Success)
            {
                s.Views = ParseCount(m.Groups[1].Value);
            }

            m = Regex.Match(html, "\"content\":\"([\\d.,\\s]+)\\s*videos?\"", RegexOptions.IgnoreCase);
            if (!m.Success)
            {
                m = Regex.Match(html, "\\b([\\d.,]+)\\s*videos?\\b", RegexOptions.IgnoreCase);
            }
            if (m.Success)
            {
                s.Videos = ParseCount(m.Groups[1].Value);
            }

            if (!s.Views.HasValue)
            {
                m = Regex.Match(html, "([\\d.]+)\\s*weergaven", RegexOptions.IgnoreCase);
                if (m.Success)
                {
                    s.Views = ParseCountDutchInt(m.Groups[1].Value);
                }
            }

            if (!s.Videos.HasValue)
            {
                m = Regex.Match(html, "([\\d.]+)\\s*video'?s?", RegexOptions.IgnoreCase);
                if (m.Success)
                {
                    s.Videos = ParseCount(m.Groups[1].Value);
                }
            }
        }

        private static async Task<string?> ResolveChannelIdAsync(string handle, CancellationToken ct)
        {
            try
            {
                string html = await Http.GetStringAsync(
                    "https://www.youtube.com/@" + Uri.EscapeDataString(handle) + "?hl=en&gl=US", ct);
                var m = Regex.Match(html, "\"channelId\":\"(UC[^\"]+)\"");
                if (m.Success)
                {
                    return m.Groups[1].Value;
                }
                m = Regex.Match(html, "\"browseId\":\"(UC[^\"]+)\"");
                if (m.Success)
                {
                    return m.Groups[1].Value;
                }
                m = Regex.Match(html, "\"externalId\":\"(UC[^\"]+)\"");
                if (m.Success)
                {
                    return m.Groups[1].Value;
                }
            }
            catch
            {
                // fall through to the InnerTube lookup
            }

            try
            {
                string body =
                    "{\"context\":{\"client\":{\"clientName\":\"WEB\",\"clientVersion\":\"" + WebClientVer +
                    "\",\"hl\":\"en\",\"gl\":\"US\"}},\"url\":\"https://www.youtube.com/@" + handle + "\"}";
                using var req = new HttpRequestMessage(HttpMethod.Post,
                    "https://www.youtube.com/youtubei/v1/navigation/resolve_url?key=" + InnertubeKey)
                {
                    Content = new StringContent(body, Encoding.UTF8, "application/json")
                };
                var resp = await Http.SendAsync(req, ct);
                var m = Regex.Match(await resp.Content.ReadAsStringAsync(ct), "\"browseId\":\"(UC[^\"]+)\"");
                if (m.Success)
                {
                    return m.Groups[1].Value;
                }
            }
            catch
            {
                // no id available
            }

            return null;
        }

        /// <summary>Tries the InnerTube channel variants until one yields subscribers and views.</summary>
        private static async Task TryInnertubeChannelAsync(ChannelStats s, string channelId,
            List<string> failures, CancellationToken ct)
        {
            var attempts = new (string Label, string Params, string Client, string Version)[]
            {
                ("innertube-about", AboutParams, "WEB", WebClientVer),
                ("innertube-browse", "", "WEB", WebClientVer),
                ("innertube-mweb", AboutParams, "MWEB", WebClientVer)
            };

            foreach (var (label, parms, client, version) in attempts)
            {
                try
                {
                    string json = await PostBrowseAsync(channelId, parms, client, version, ct);
                    if (string.IsNullOrWhiteSpace(json))
                    {
                        failures.Add(label + ": leeg antwoord");
                        SourceHealth.Record("kanaal", "@" + s.Handle, label, SourceHealth.Empty, "leeg antwoord");
                        continue;
                    }

                    ParseChannelJson(json, s);
                    if (HasNumbers(s))
                    {
                        s.Source = label;
                        SourceHealth.Record("kanaal", "@" + s.Handle, label, SourceHealth.Ok, Detail(s));
                        return;
                    }

                    s.FormatChanged = true;
                    failures.Add(label + ": cijfers niet gevonden");
                    SourceHealth.Record("kanaal", "@" + s.Handle, label, SourceHealth.Empty,
                        "cijfers niet gevonden");
                }
                catch (Exception ex)
                {
                    failures.Add(label + ": " + ex.Message);
                    SourceHealth.Record("kanaal", "@" + s.Handle, label, SourceHealth.Failed, ex.Message);
                }
            }
        }

        private static async Task<string> PostBrowseAsync(string channelId, string parms,
            string client, string version, CancellationToken ct)
        {
            string body = "{\"context\":{\"client\":{\"clientName\":\"" + client +
                          "\",\"clientVersion\":\"" + version + "\",\"hl\":\"en\",\"gl\":\"US\"}},\"browseId\":\"" +
                          channelId + "\"" +
                          (string.IsNullOrEmpty(parms) ? "" : ",\"params\":\"" + parms + "\"") + "}";

            using var req = new HttpRequestMessage(HttpMethod.Post,
                $"https://www.youtube.com/youtubei/v1/browse?key={InnertubeKey}&prettyPrint=false&t={Now()}")
            {
                Content = new StringContent(body, Encoding.UTF8, "application/json")
            };
            req.Headers.TryAddWithoutValidation("X-Youtube-Client-Name", client == "MWEB" ? "2" : "1");
            req.Headers.TryAddWithoutValidation("X-Youtube-Client-Version", version);

            var resp = await Http.SendAsync(req, ct);
            if (!resp.IsSuccessStatusCode)
            {
                throw new HttpRequestException("HTTP " + (int)resp.StatusCode);
            }
            return await resp.Content.ReadAsStringAsync(ct);
        }

        /// <summary>Shared InnerTube channel parser: every known shape of the three numbers.</summary>
        private static void ParseChannelJson(string json, ChannelStats s)
        {
            if (string.IsNullOrEmpty(s.Title)
                || s.Title.Contains("Before you continue", StringComparison.OrdinalIgnoreCase))
            {
                var title = Regex.Match(json, "\"channelMetadataRenderer\":\\{\"title\":\"([^\"]+)\"");
                if (!title.Success)
                {
                    title = Regex.Match(json, "\"title\":\"([^\"]+)\"");
                }
                if (title.Success)
                {
                    s.Title = Unescape(title.Groups[1].Value);
                }
            }

            if (!s.Subscribers.HasValue)
            {
                TryParseSubscribers(json, s, requireOwnHeader: false);
            }

            if (!s.Views.HasValue)
            {
                var m = Regex.Match(json, "\"viewCountText\":\"([^\"]+)\"");
                if (!m.Success)
                {
                    m = Regex.Match(json, "\"viewCountText\":\\{[^}]*?\"simpleText\":\"([^\"]+)\"");
                }
                if (!m.Success)
                {
                    m = Regex.Match(json,
                        "\"videoViewCountRenderer\":\\{\"viewCount\":\\{\"simpleText\":\"([^\"]+)\"");
                }
                if (m.Success)
                {
                    s.Views = ParseCount(m.Groups[1].Value);
                }
            }

            if (!s.Videos.HasValue)
            {
                var m = Regex.Match(json, "\"content\":\"([\\d.,\\s]+)\\s*videos?\"", RegexOptions.IgnoreCase);
                if (!m.Success)
                {
                    m = Regex.Match(json, "\"videoCountText\":\\{[^}]*?\"simpleText\":\"([^\"]+)\"");
                }
                if (m.Success)
                {
                    s.Videos = ParseCount(m.Groups[1].Value);
                }
            }
        }

        public static async Task<VideoStats> FetchVideoAsync(string videoId, CancellationToken ct = default)
        {
            videoId = ExtractVideoId(videoId) ?? videoId;
            var v = new VideoStats { VideoId = videoId };
            var failures = new List<string>();

            // Source 1: the WEB player response.
            await TryPlayerAsync(v, videoId, "WEB", WebClientVer, "player", failures, ct);

            // Source 2: the watch page HTML.
            if (!v.Views.HasValue)
            {
                try
                {
                    string html = await Http.GetStringAsync(
                        "https://www.youtube.com/watch?v=" + videoId + "&hl=en&gl=US&_t=" + Now(), ct);

                    var m = Regex.Match(html, "\"viewCount\":\"(\\d+)\"");
                    if (!m.Success)
                    {
                        m = Regex.Match(html, "\"viewCount\":\\{\"simpleText\":\"([\\d.,]+)");
                    }
                    if (m.Success)
                    {
                        v.Views = ParseCount(m.Groups[1].Value);
                        v.Source = "watch-html";
                        SourceHealth.Record("video", videoId, "watch-html", SourceHealth.Ok,
                            "views=" + (v.Views?.ToString(CultureInfo.InvariantCulture) ?? "-"));
                    }
                    else
                    {
                        v.FormatChanged = true;
                        failures.Add("watch-html: viewCount niet gevonden");
                        SourceHealth.Record("video", videoId, "watch-html", SourceHealth.Empty,
                            "viewCount niet gevonden");
                    }

                    if (string.IsNullOrEmpty(v.Title))
                    {
                        m = Regex.Match(html, "\"title\":\"([^\"]+)\"");
                        if (m.Success)
                        {
                            v.Title = Unescape(m.Groups[1].Value);
                        }
                    }
                }
                catch (Exception ex)
                {
                    failures.Add("watch-html: " + ex.Message);
                    SourceHealth.Record("video", videoId, "watch-html", SourceHealth.Failed, ex.Message);
                }
            }

            // Source 3: the ANDROID player, which normally survives WEB payload changes.
            if (!v.Views.HasValue)
            {
                await TryPlayerAsync(v, videoId, "ANDROID", AndroidClientVer, "player-android", failures, ct);
            }

            if (v.Views.HasValue)
            {
                v.Error = "";
            }
            else
            {
                v.Error = failures.Count > 0 ? string.Join(" | ", failures) : "geen views gevonden";
                if (v.FormatChanged)
                {
                    v.Error += " (formaat gewijzigd?)";
                }
            }

            return v;
        }

        private static async Task TryPlayerAsync(VideoStats v, string videoId, string client,
            string version, string label, List<string> failures, CancellationToken ct)
        {
            try
            {
                using var req = new HttpRequestMessage(HttpMethod.Post,
                    $"https://www.youtube.com/youtubei/v1/player?key={InnertubeKey}&prettyPrint=false&t={Now()}")
                {
                    Content = new StringContent(PlayerPayload(videoId, client, version), Encoding.UTF8,
                        "application/json")
                };
                req.Headers.TryAddWithoutValidation("X-Youtube-Client-Name", client == "ANDROID" ? "3" : "1");
                req.Headers.TryAddWithoutValidation("X-Youtube-Client-Version", version);

                using var resp = await Http.SendAsync(req, ct);
                string json = await resp.Content.ReadAsStringAsync(ct);
                using var doc = JsonDocument.Parse(json);

                bool gotViews = false;
                if (doc.RootElement.TryGetProperty("videoDetails", out var details))
                {
                    if (string.IsNullOrEmpty(v.Title) && details.TryGetProperty("title", out var title))
                    {
                        v.Title = title.GetString() ?? "";
                    }
                    if (details.TryGetProperty("viewCount", out var views))
                    {
                        string? raw = views.ValueKind == JsonValueKind.String ? views.GetString() : views.ToString();
                        if (long.TryParse(raw, out long parsed))
                        {
                            v.Views = parsed;
                            v.Source = label;
                            gotViews = true;
                            SourceHealth.Record("video", videoId, label, SourceHealth.Ok,
                                "views=" + parsed.ToString(CultureInfo.InvariantCulture));
                        }
                    }
                }

                if (!gotViews)
                {
                    v.FormatChanged = true;
                    failures.Add(label + ": viewCount ontbreekt");
                    SourceHealth.Record("video", videoId, label, SourceHealth.Empty, "viewCount ontbreekt");
                }
            }
            catch (Exception ex)
            {
                failures.Add(label + ": " + ex.Message);
                SourceHealth.Record("video", videoId, label, SourceHealth.Failed, ex.Message);
            }
        }

        private static string PlayerPayload(string videoId, string client, string version)
        {
            string extra = client == "ANDROID" ? ",\"androidSdkVersion\":30" : "";
            return "{\"context\":{\"client\":{\"clientName\":\"" + client + "\",\"clientVersion\":\"" + version +
                   "\",\"hl\":\"en\",\"gl\":\"US\"" + extra + "}},\"videoId\":\"" + videoId + "\"}";
        }

        public static string? ExtractVideoId(string? s)
        {
            if (string.IsNullOrWhiteSpace(s))
            {
                return null;
            }
            s = s.Trim();
            if (Regex.IsMatch(s, "^[a-zA-Z0-9_-]{11}$"))
            {
                return s;
            }
            var m = Regex.Match(s, "(?:v=|/shorts/|/live/|youtu\\.be/)([a-zA-Z0-9_-]{11})");
            return m.Success ? m.Groups[1].Value : null;
        }

        /// <summary>
        /// Reads the channel's own subscriber count. A channel that hides its subscriber count publishes
        /// no subscriber text at all, so it must stay unknown rather than picking up the count of a
        /// featured or related channel rendered on the same page.
        /// </summary>
        /// <param name="requireOwnHeader">
        /// Set for HTML pages, where related channels are part of the document. Their cards carry a
        /// subscriber count but no channel-wide view total, which is what marks the owner's own line.
        /// </param>
        private static void TryParseSubscribers(string payload, ChannelStats s, bool requireOwnHeader)
        {
            string[] patterns =
            {
                "\"subscriberCountText\":\"([^\"]+)\"",
                "\"subscriberCountText\":\\{[^}]*?\"simpleText\":\"([^\"]+)\"",
                "\"content\":\"([\\d.,]+\\s*(?:[KMB]|thousand|million|billion)?\\s*subscribers?)\"",
                "\"accessibilityLabel\":\"([\\d.,]+\\s*(?:thousand|million|K|M)?[^\"]*subscribers?)\""
            };

            foreach (string pattern in patterns)
            {
                foreach (Match m in Regex.Matches(payload, pattern, RegexOptions.IgnoreCase))
                {
                    if (requireOwnHeader && !LooksLikeOwnHeader(payload, m.Index))
                    {
                        continue;
                    }

                    string raw = m.Groups[1].Value;
                    long? count = ParseCount(NormalizeCountText(raw));
                    if (!count.HasValue)
                    {
                        continue;
                    }

                    s.SubsText = NormalizeCountText(raw);
                    s.Subscribers = count;
                    s.SubsApprox = Regex.IsMatch(raw, "[KMBkmb]");
                    return;
                }
            }
        }

        /// <summary>
        /// The channel's own subscriber line is followed by its channel-wide view total; the cards of
        /// featured and related channels have no such total, so this check keeps somebody else's number
        /// out of the overlay.
        /// </summary>
        private static bool LooksLikeOwnHeader(string payload, int index)
        {
            int length = Math.Min(240, payload.Length - index);
            return payload.IndexOf("\"viewCountText\"", index, length, StringComparison.Ordinal) >= 0;
        }

        private static string NormalizeCountText(string value) =>
            value.Replace("thousand subscribers", "K")
                 .Replace("thousand", "K")
                 .Replace("million subscribers", "M")
                 .Replace("million", "M")
                 .Replace("billion", "B");

        public static long? ParseCount(string text)
        {
            if (string.IsNullOrWhiteSpace(text))
            {
                return null;
            }
            string s = text.Replace("\u00a0", " ").Trim();
            s = s.Replace("thousand", "K", StringComparison.OrdinalIgnoreCase)
                 .Replace("million", "M", StringComparison.OrdinalIgnoreCase)
                 .Replace("billion", "B", StringComparison.OrdinalIgnoreCase);

            var m = Regex.Match(s, "([\\d.,]+)\\s*([KMB])\\b", RegexOptions.IgnoreCase);
            if (m.Success)
            {
                string num = m.Groups[1].Value;
                if (num.Contains(',') && !num.Contains('.'))
                {
                    num = num.Replace(',', '.');
                }
                else if (num.Contains(',') && num.Contains('.'))
                {
                    num = num.Replace(",", "");
                }
                if (!double.TryParse(num, NumberStyles.Float, CultureInfo.InvariantCulture, out double val))
                {
                    return null;
                }
                double mult = m.Groups[2].Value.ToUpperInvariant() switch
                {
                    "K" => 1_000d,
                    "M" => 1_000_000d,
                    "B" => 1_000_000_000d,
                    _ => 1d
                };
                return (long)(val * mult);
            }

            return ParseCountDutchInt(Regex.Match(s, "[\\d.,]+").Value);
        }

        /// <summary>Parses "1.234.567" (Dutch) and "1,234,567" (English) alike.</summary>
        public static long? ParseCountDutchInt(string raw)
        {
            if (string.IsNullOrWhiteSpace(raw))
            {
                return null;
            }
            string s = raw.Trim();

            if (s.Contains(',') && s.Contains('.'))
            {
                s = s.LastIndexOf('.') <= s.LastIndexOf(',')
                    ? s.Replace(".", "").Replace(',', '.')
                    : s.Replace(",", "");
            }
            else if (s.Contains('.'))
            {
                string[] parts = s.Split('.');
                if (parts.Length == 2 && parts[1].Length == 3 && parts[0].All(char.IsDigit))
                {
                    s = parts[0] + parts[1];
                }
                else
                {
                    s = s.Replace(".", "");
                }
            }
            else if (s.Contains(','))
            {
                string[] parts = s.Split(',');
                if (parts.Length == 2 && parts[1].Length == 3 && parts[0].All(char.IsDigit))
                {
                    s = parts[0] + parts[1];
                }
                else if (parts.Length != 2 || parts[1].Length > 2)
                {
                    s = s.Replace(",", "");
                }
                else
                {
                    s = parts[0] + "." + parts[1];
                }
            }

            if (long.TryParse(s, NumberStyles.Integer, CultureInfo.InvariantCulture, out long l))
            {
                return l;
            }
            if (double.TryParse(s, NumberStyles.Float, CultureInfo.InvariantCulture, out double d))
            {
                return (long)d;
            }
            if (!long.TryParse(Regex.Replace(raw, "[^\\d]", ""), out long digits))
            {
                return null;
            }
            return digits;
        }

        public static string Fmt(long? n, bool approx = false)
        {
            if (!n.HasValue)
            {
                return "\u2014";
            }
            return n.Value.ToString("N0", CultureInfo.GetCultureInfo("nl-NL"));
        }

        private static string Unescape(string s)
        {
            try
            {
                return Regex.Unescape(s);
            }
            catch
            {
                return s;
            }
        }
    }
}
