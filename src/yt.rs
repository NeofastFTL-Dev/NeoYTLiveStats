//! Reads live numbers off YouTube.
//!
//! Every number has more than one source, so a markup or payload change turns into a fallback
//! instead of a blank screen:
//!   channel: about page HTML -> InnerTube browse (about params) -> InnerTube browse (plain) -> InnerTube MWEB
//!   video:   InnerTube WEB player -> watch page HTML -> InnerTube ANDROID player
//!
//! When a source answers but its payload cannot be parsed, `format_changed` is set and the
//! failure is listed in `error`, which is what distinguishes "YouTube changed something" from
//! "the network is down".

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::Regex;

use crate::health;

const INNERTUBE_KEY: &str = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Wa8ESU4OY";
const WEB_CLIENT_VER: &str = "2.20261026.01.00";
const ANDROID_CLIENT_VER: &str = "19.09.37";
const KNOWN_MAIN_CHANNEL_ID: &str = "UCYLki_57xkd9KoKygzB0q3A";
const ABOUT_PARAMS: &str = "EgVhYm91dA%3D%3D";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const COOKIE: &str = "CONSENT=YES+cb.20210328-17-p0.en+FX+410; SOCS=CAISNQgDEitib3FfaWRlbnRpdHlmcm9udGVuZHVpc2VydmVyXzIwMjMwODI5LjA3X3AxGgJlbiACGgYIgLC_pwY";

#[derive(Debug, Clone, Default)]
pub struct ChannelStats {
    pub handle: String,
    pub title: String,
    pub channel_id: String,
    pub subscribers: Option<i64>,
    pub subs_approx: bool,
    pub subs_text: String,
    pub views: Option<i64>,
    pub videos: Option<i64>,
    pub error: String,
    /// Source that produced the numbers ("html", "innertube-about", ...), empty when none did.
    pub source: String,
    /// True when a source responded but the payload had no numbers we recognise.
    pub format_changed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VideoStats {
    pub video_id: String,
    pub title: String,
    pub views: Option<i64>,
    pub error: String,
    /// Source that produced the numbers ("player", "watch-html", "player-android").
    pub source: String,
    pub format_changed: bool,
}

static CHACHED_CHANNEL_IDS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent(USER_AGENT)
        .build()
}

fn get(url: &str) -> Result<String, String> {
    agent()
        .get(url)
        .set("Accept-Language", "en-US,en;q=0.9")
        .set("Accept", "text/html,application/xhtml+xml,application/json")
        .set("Cache-Control", "no-cache, no-store, must-revalidate")
        .set("Pragma", "no-cache")
        .set("Cookie", COOKIE)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
}

fn post_json(url: &str, body: &str, client_name: &str, client_version: &str) -> Result<String, String> {
    let request = agent()
        .post(url)
        .set("Content-Type", "application/json")
        .set("Accept-Language", "en-US,en;q=0.9")
        .set("Cookie", COOKIE)
        .set("X-Youtube-Client-Name", client_name)
        .set("X-Youtube-Client-Version", client_version);

    match request.send_string(body) {
        Ok(response) => response.into_string().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(code, _)) => Err(format!("HTTP {code}")),
        Err(other) => Err(other.to_string()),
    }
}

/// Percent-encodes everything a handle could contain that would break the URL.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn fetch_channel(handle: &str) -> ChannelStats {
    let handle = handle.trim().trim_start_matches('@').trim().to_string();
    let mut stats = ChannelStats {
        handle: handle.clone(),
        ..Default::default()
    };
    let mut failures: Vec<String> = Vec::new();

    // Source 1: the public channel about page.
    let mut html_answered = false;
    match get(&format!(
        "https://www.youtube.com/@{}/about?hl=en&gl=US&_t={}",
        escape(&handle),
        now_ms()
    )) {
        Ok(html) => {
            html_answered = true;
            parse_about_html(&html, &mut stats);
            if !stats.channel_id.is_empty() {
                remember_channel_id(&handle, &stats.channel_id);
            }

            if has_numbers(&stats) {
                stats.source = "html".to_string();
                health::record("kanaal", &format!("@{handle}"), "html", health::OK, &detail(&stats));
            } else if stats.title.eq_ignore_ascii_case("Before you continue to YouTube") {
                failures.push("html: toestemmingspagina".to_string());
                health::record("kanaal", &format!("@{handle}"), "html", health::EMPTY, "toestemmingspagina");
            } else {
                stats.format_changed = true;
                failures.push("html: cijfers niet in de pagina".to_string());
                health::record("kanaal", &format!("@{handle}"), "html", health::EMPTY, "cijfers niet in de pagina");
            }
        }
        Err(err) => {
            failures.push(format!("html: {err}"));
            health::record("kanaal", &format!("@{handle}"), "html", health::FAILED, &err);
        }
    }

    // Description text is never validated; only the about page carries one.
    // Source 2..4: InnerTube, with the channel id from the page, the cache or the resolve endpoint.
    if !has_numbers(&stats) {
        let mut channel_id = stats.channel_id.clone();
        if channel_id.is_empty() {
            channel_id = cached_channel_id(&handle).unwrap_or_default();
        }
        if channel_id.is_empty() && handle.eq_ignore_ascii_case("NeofastFTL_YT") {
            channel_id = KNOWN_MAIN_CHANNEL_ID.to_string();
        }
        if channel_id.is_empty() {
            channel_id = resolve_channel_id(&handle).unwrap_or_default();
        }

        if channel_id.is_empty() {
            failures.push("kanaal-id niet gevonden".to_string());
        } else {
            stats.channel_id = channel_id.clone();
            remember_channel_id(&handle, &channel_id);
            try_innertube_channel(&mut stats, &channel_id, &mut failures);
        }
    }

    let low_views = stats.views.map(|v| v < 1000).unwrap_or(false) && handle.eq_ignore_ascii_case("NeofastFTL_YT");

    if has_numbers(&stats) {
        if low_views {
            stats.error = format!("Views te laag (waarschijnlijk video i.p.v. kanaal): {}", stats.views.unwrap_or(0));
        } else if stats.subscribers.is_none() {
            let source = if stats.source.is_empty() { "?" } else { &stats.source };
            stats.error = format!("Geen subscribers in deze bron ({source})");
        } else if stats.views.is_none() {
            let source = if stats.source.is_empty() { "?" } else { &stats.source };
            stats.error = format!("Geen kanaal views in deze bron ({source})");
        } else {
            stats.error.clear();
        }
    } else {
        if low_views {
            failures.push(format!("views te laag: {}", stats.views.unwrap_or(0)));
        }
        if html_answered {
            stats.format_changed = true;
        }
        stats.error = if failures.is_empty() {
            "geen kanaalcijfers".to_string()
        } else {
            failures.join(" | ")
        };
        if stats.format_changed && !failures.is_empty() {
            stats.error.push_str(" (formaat gewijzigd?)");
        }
    }

    stats
}

/// True when a source produced at least one of the numbers the overlay needs. InnerTube does
/// not carry the channel-wide view total at all, so requiring both would make every fallback
/// look broken; the missing piece is reported instead.
fn has_numbers(stats: &ChannelStats) -> bool {
    stats.subscribers.is_some() || stats.views.is_some()
}

fn detail(stats: &ChannelStats) -> String {
    format!(
        "subs={} views={}",
        stats.subscribers.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
        stats.views.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string())
    )
}

fn remember_channel_id(handle: &str, id: &str) {
    if let Ok(mut cache) = CHACHED_CHANNEL_IDS.lock() {
        cache.insert(handle.to_ascii_lowercase(), id.to_string());
    }
}

fn cached_channel_id(handle: &str) -> Option<String> {
    CHACHED_CHANNEL_IDS
        .lock()
        .ok()
        .and_then(|cache| cache.get(&handle.to_ascii_lowercase()).cloned())
}

fn parse_about_html(html: &str, stats: &mut ChannelStats) {
    static EXTERNAL_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""externalId":"(UC[^"]+)""#).unwrap());
    static CHANNEL_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""channelId":"(UC[^"]+)""#).unwrap());
    static META_TITLE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""channelMetadataRenderer":\{"title":"([^"]+)""#).unwrap());
    static PAGE_TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<title>([^<]+)</title>").unwrap());
    static VIEW_TEXT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""viewCountText":"([^"]+)""#).unwrap());
    static VIEW_SIMPLE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""viewCountText":\{[^}]*?"simpleText":"([^"]+)""#).unwrap());
    static VIDEO_CONTENT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?i)"content":"([\d.,\s]+)\s*videos?""#).unwrap());
    static VIDEO_BARE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b([\d.,]+)\s*videos?\b").unwrap());
    static DUTCH_VIEWS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)([\d.]+)\s*weergaven").unwrap());
    static DUTCH_VIDEOS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)([\d.]+)\s*video'?s?").unwrap());

    if let Some(m) = EXTERNAL_ID.captures(html).or_else(|| CHANNEL_ID.captures(html)) {
        stats.channel_id = m[1].to_string();
    }

    if let Some(m) = META_TITLE.captures(html) {
        stats.title = unescape(&m[1]);
    }
    if stats.title.is_empty() {
        if let Some(m) = PAGE_TITLE.captures(html) {
            stats.title = m[1].replace(" - YouTube", "").trim().to_string();
        }
    }

    try_parse_subscribers(html, stats, true);

    if let Some(m) = VIEW_TEXT.captures(html).or_else(|| VIEW_SIMPLE.captures(html)) {
        stats.views = parse_count(&m[1]);
    }

    if let Some(m) = VIDEO_CONTENT.captures(html).or_else(|| VIDEO_BARE.captures(html)) {
        stats.videos = parse_count(&m[1]);
    }

    if stats.views.is_none() {
        if let Some(m) = DUTCH_VIEWS.captures(html) {
            stats.views = parse_count_dutch_int(&m[1]);
        }
    }

    if stats.videos.is_none() {
        if let Some(m) = DUTCH_VIDEOS.captures(html) {
            stats.videos = parse_count(&m[1]);
        }
    }
}

fn resolve_channel_id(handle: &str) -> Option<String> {
    static CHANNEL_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""channelId":"(UC[^"]+)""#).unwrap());
    static BROWSE_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""browseId":"(UC[^"]+)""#).unwrap());
    static EXTERNAL_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""externalId":"(UC[^"]+)""#).unwrap());

    if let Ok(html) = get(&format!(
        "https://www.youtube.com/@{}?hl=en&gl=US",
        escape(handle)
    )) {
        if let Some(m) = CHANNEL_ID.captures(&html).or_else(|| BROWSE_ID.captures(&html)).or_else(|| EXTERNAL_ID.captures(&html)) {
            return Some(m[1].to_string());
        }
    }

    let body = format!(
        r#"{{"context":{{"client":{{"clientName":"WEB","clientVersion":"{WEB_CLIENT_VER}","hl":"en","gl":"US"}}}},"url":"https://www.youtube.com/@{handle}"}}"#
    );

    if let Ok(text) = post_json(
        &format!("https://www.youtube.com/youtubei/v1/navigation/resolve_url?key={INNERTUBE_KEY}"),
        &body,
        "1",
        WEB_CLIENT_VER,
    ) {
        if let Some(m) = BROWSE_ID.captures(&text) {
            return Some(m[1].to_string());
        }
    }

    None
}

/// Tries the InnerTube channel variants until one yields subscribers and views.
fn try_innertube_channel(stats: &mut ChannelStats, channel_id: &str, failures: &mut Vec<String>) {
    let attempts = [
        ("innertube-about", ABOUT_PARAMS, "WEB", WEB_CLIENT_VER, "1"),
        ("innertube-browse", "", "WEB", WEB_CLIENT_VER, "1"),
        ("innertube-mweb", ABOUT_PARAMS, "MWEB", WEB_CLIENT_VER, "2"),
    ];

    for (label, params, client, version, client_name) in attempts {
        match post_browse(channel_id, params, client, version, client_name) {
            Ok(json) if json.trim().is_empty() => {
                failures.push(format!("{label}: leeg antwoord"));
                health::record("kanaal", &format!("@{}", stats.handle), label, health::EMPTY, "leeg antwoord");
            }
            Ok(json) => {
                parse_channel_json(&json, stats);
                if has_numbers(stats) {
                    stats.source = label.to_string();
                    health::record("kanaal", &format!("@{}", stats.handle), label, health::OK, &detail(stats));
                    return;
                }

                stats.format_changed = true;
                failures.push(format!("{label}: cijfers niet gevonden"));
                health::record("kanaal", &format!("@{}", stats.handle), label, health::EMPTY, "cijfers niet gevonden");
            }
            Err(err) => {
                failures.push(format!("{label}: {err}"));
                health::record("kanaal", &format!("@{}", stats.handle), label, health::FAILED, &err);
            }
        }
    }
}

fn post_browse(channel_id: &str, params: &str, client: &str, version: &str, client_name: &str) -> Result<String, String> {
    let params_fragment = if params.is_empty() {
        String::new()
    } else {
        format!(r#","params":"{params}""#)
    };
    let body = format!(
        r#"{{"context":{{"client":{{"clientName":"{client}","clientVersion":"{version}","hl":"en","gl":"US"}}}},"browseId":"{channel_id}"{params_fragment}}}"#
    );

    post_json(
        &format!(
            "https://www.youtube.com/youtubei/v1/browse?key={INNERTUBE_KEY}&prettyPrint=false&t={}",
            now_ms()
        ),
        &body,
        client_name,
        version,
    )
}

/// Shared InnerTube channel parser: every known shape of the three numbers.
fn parse_channel_json(json: &str, stats: &mut ChannelStats) {
    static META_TITLE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""channelMetadataRenderer":\{"title":"([^"]+)""#).unwrap());
    static TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""title":"([^"]+)""#).unwrap());
    static VIEW_TEXT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""viewCountText":"([^"]+)""#).unwrap());
    static VIEW_SIMPLE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""viewCountText":\{[^}]*?"simpleText":"([^"]+)""#).unwrap());
    static VIDEO_VIEW_COUNT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""videoViewCountRenderer":\{"viewCount":\{"simpleText":"([^"]+)""#).unwrap());
    static VIDEO_CONTENT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?i)"content":"([\d.,\s]+)\s*videos?""#).unwrap());
    static VIDEO_COUNT_TEXT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""videoCountText":\{[^}]*?"simpleText":"([^"]+)""#).unwrap());

    if stats.title.is_empty() || contains_ci(&stats.title, "Before you continue") {
        if let Some(m) = META_TITLE.captures(json).or_else(|| TITLE.captures(json)) {
            stats.title = unescape(&m[1]);
        }
    }

    if stats.subscribers.is_none() {
        try_parse_subscribers(json, stats, false);
    }

    if stats.views.is_none() {
        if let Some(m) = VIEW_TEXT
            .captures(json)
            .or_else(|| VIEW_SIMPLE.captures(json))
            .or_else(|| VIDEO_VIEW_COUNT.captures(json))
        {
            stats.views = parse_count(&m[1]);
        }
    }

    if stats.videos.is_none() {
        if let Some(m) = VIDEO_CONTENT.captures(json).or_else(|| VIDEO_COUNT_TEXT.captures(json)) {
            stats.videos = parse_count(&m[1]);
        }
    }
}

pub fn fetch_video(video_id: &str) -> VideoStats {
    let video_id = extract_video_id(video_id).unwrap_or_else(|| video_id.trim().to_string());
    let mut stats = VideoStats {
        video_id: video_id.clone(),
        ..Default::default()
    };
    let mut failures: Vec<String> = Vec::new();

    // Source 1: the WEB player response.
    try_player(&mut stats, &video_id, "WEB", WEB_CLIENT_VER, "player", "1", &mut failures);

    // Source 2: the watch page HTML.
    if stats.views.is_none() {
        match get(&format!(
            "https://www.youtube.com/watch?v={video_id}&hl=en&gl=US&_t={}",
            now_ms()
        )) {
            Ok(html) => {
                static VIEW_COUNT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""viewCount":"(\d+)""#).unwrap());
                static VIEW_SIMPLE: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r#""viewCount":\{"simpleText":"([\d.,]+)"#).unwrap());
                static TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""title":"([^"]+)""#).unwrap());

                if let Some(m) = VIEW_COUNT.captures(&html).or_else(|| VIEW_SIMPLE.captures(&html)) {
                    stats.views = parse_count(&m[1]);
                    stats.source = "watch-html".to_string();
                    health::record(
                        "video",
                        &video_id,
                        "watch-html",
                        health::OK,
                        &format!("views={}", stats.views.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string())),
                    );
                } else {
                    stats.format_changed = true;
                    failures.push("watch-html: viewCount niet gevonden".to_string());
                    health::record("video", &video_id, "watch-html", health::EMPTY, "viewCount niet gevonden");
                }

                if stats.title.is_empty() {
                    if let Some(m) = TITLE.captures(&html) {
                        stats.title = unescape(&m[1]);
                    }
                }
            }
            Err(err) => {
                failures.push(format!("watch-html: {err}"));
                health::record("video", &video_id, "watch-html", health::FAILED, &err);
            }
        }
    }

    // Source 3: the ANDROID player, which normally survives WEB payload changes.
    if stats.views.is_none() {
        try_player(&mut stats, &video_id, "ANDROID", ANDROID_CLIENT_VER, "player-android", "3", &mut failures);
    }

    if stats.views.is_some() {
        stats.error.clear();
    } else {
        stats.error = if failures.is_empty() {
            "geen views gevonden".to_string()
        } else {
            failures.join(" | ")
        };
        if stats.format_changed {
            stats.error.push_str(" (formaat gewijzigd?)");
        }
    }

    stats
}

fn try_player(
    stats: &mut VideoStats,
    video_id: &str,
    client: &str,
    version: &str,
    label: &str,
    client_name: &str,
    failures: &mut Vec<String>,
) {
    let extra = if client == "ANDROID" { r#","androidSdkVersion":30"# } else { "" };
    let body = format!(
        r#"{{"context":{{"client":{{"clientName":"{client}","clientVersion":"{version}","hl":"en","gl":"US"{extra}}}}},"videoId":"{video_id}"}}"#
    );

    let result = post_json(
        &format!(
            "https://www.youtube.com/youtubei/v1/player?key={INNERTUBE_KEY}&prettyPrint=false&t={}",
            now_ms()
        ),
        &body,
        client_name,
        version,
    );

    match result {
        Ok(json) => {
            let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or(serde_json::Value::Null);
            let mut got_views = false;

            if let Some(details) = parsed.get("videoDetails") {
                if stats.title.is_empty() {
                    if let Some(title) = details.get("title").and_then(|t| t.as_str()) {
                        stats.title = title.to_string();
                    }
                }
                if let Some(views) = details.get("viewCount") {
                    let raw = views
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| views.to_string());
                    if let Ok(count) = raw.trim().parse::<i64>() {
                        stats.views = Some(count);
                        stats.source = label.to_string();
                        got_views = true;
                        health::record("video", video_id, label, health::OK, &format!("views={count}"));
                    }
                }
            }

            if !got_views {
                stats.format_changed = true;
                failures.push(format!("{label}: viewCount ontbreekt"));
                health::record("video", video_id, label, health::EMPTY, "viewCount ontbreekt");
            }
        }
        Err(err) => {
            failures.push(format!("{label}: {err}"));
            health::record("video", video_id, label, health::FAILED, &err);
        }
    }
}

pub fn extract_video_id(value: &str) -> Option<String> {
    static BARE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_-]{11}$").unwrap());
    static LINK: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:v=|/shorts/|/live/|youtu\.be/)([A-Za-z0-9_-]{11})").unwrap());

    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if BARE.is_match(value) {
        return Some(value.to_string());
    }
    LINK.captures(value).map(|m| m[1].to_string())
}

/// Reads the channel's own subscriber count. A channel that hides its subscriber count
/// publishes no subscriber text at all, so it must stay unknown rather than picking up the
/// count of a featured or related channel rendered on the same page.
fn try_parse_subscribers(payload: &str, stats: &mut ChannelStats, require_own_header: bool) {
    static PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        [
            r#""subscriberCountText":"([^"]+)""#,
            r#""subscriberCountText":\{[^}]*?"simpleText":"([^"]+)""#,
            r#"(?i)"content":"([\d.,]+\s*(?:[KMB]|thousand|million|billion)?\s*subscribers?)""#,
            r#"(?i)"accessibilityLabel":"([\d.,]+\s*(?:thousand|million|K|M)?[^"]*subscribers?)""#,
        ]
        .iter()
        .map(|pattern| Regex::new(pattern).unwrap())
        .collect()
    });

    for pattern in PATTERNS.iter() {
        for capture in pattern.captures_iter(payload) {
            let whole = capture.get(0).map(|m| m.start()).unwrap_or(0);
            if require_own_header && !looks_like_own_header(payload, whole) {
                continue;
            }

            let raw = capture[1].to_string();
            let normalized = normalize_count_text(&raw);
            if let Some(count) = parse_count(&normalized) {
                stats.subs_text = normalized;
                stats.subscribers = Some(count);
                stats.subs_approx = raw.chars().any(|c| matches!(c, 'K' | 'M' | 'B' | 'k' | 'm' | 'b'));
                return;
            }
        }
    }
}

/// The channel's own subscriber line is followed by its channel-wide view total; the cards of
/// featured and related channels have no such total, so this check keeps somebody else's
/// number out of the overlay.
fn looks_like_own_header(payload: &str, index: usize) -> bool {
    let rest = &payload[index.min(payload.len())..];
    let window: String = rest.chars().take(240).collect();
    window.contains(r#""viewCountText""#)
}

fn normalize_count_text(value: &str) -> String {
    value
        .replace("thousand subscribers", "K")
        .replace("thousand", "K")
        .replace("million subscribers", "M")
        .replace("million", "M")
        .replace("billion", "B")
}

pub fn parse_count(text: &str) -> Option<i64> {
    static MULTIPLIER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)([\d.,]+)\s*([KMB])\b").unwrap());
    static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\d.,]+").unwrap());

    let trimmed = text.replace('\u{a0}', " ");
    let trimmed = trimmed.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut s = replace_ci(trimmed, "thousand", "K");
    s = replace_ci(&s, "million", "M");
    s = replace_ci(&s, "billion", "B");

    if let Some(capture) = MULTIPLIER.captures(&s) {
        let mut number = capture[1].to_string();
        if number.contains(',') && !number.contains('.') {
            number = number.replace(',', ".");
        } else if number.contains(',') && number.contains('.') {
            number = number.replace(',', "");
        }
        if let Ok(value) = number.parse::<f64>() {
            let multiplier = match capture[2].to_ascii_uppercase().as_str() {
                "K" => 1_000f64,
                "M" => 1_000_000f64,
                "B" => 1_000_000_000f64,
                _ => 1f64,
            };
            return Some((value * multiplier) as i64);
        }
    }

    let digits = NUMBER.find(&s).map(|m| m.as_str().to_string())?;
    parse_count_dutch_int(&digits)
}

/// Parses "1.234.567" (Dutch) and "1,234,567" (English) alike.
pub fn parse_count_dutch_int(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    let mut s = raw.to_string();

    if s.contains(',') && s.contains('.') {
        let last_dot = s.rfind('.').unwrap_or(0);
        let last_comma = s.rfind(',').unwrap_or(0);
        s = if last_dot <= last_comma {
            s.replace('.', "").replace(',', ".")
        } else {
            s.replace(',', "")
        };
    } else if s.contains('.') {
        let parts: Vec<&str> = s.split('.').collect();
        s = if parts.len() == 2 && parts[1].len() == 3 && parts[0].chars().all(|c| c.is_ascii_digit()) {
            format!("{}{}", parts[0], parts[1])
        } else {
            s.replace('.', "")
        };
    } else if s.contains(',') {
        let parts: Vec<&str> = s.split(',').collect();
        s = if parts.len() == 2 && parts[1].len() == 3 && parts[0].chars().all(|c| c.is_ascii_digit()) {
            format!("{}{}", parts[0], parts[1])
        } else if parts.len() != 2 || parts[1].len() > 2 {
            s.replace(',', "")
        } else {
            format!("{}.{}", parts[0], parts[1])
        };
    }

    if let Ok(value) = s.parse::<i64>() {
        return Some(value);
    }
    if let Ok(value) = s.parse::<f64>() {
        return Some(value as i64);
    }

    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().ok()
}

/// Dutch thousands separators, like the overlay shows them: 1.234.567.
pub fn fmt(value: Option<i64>) -> String {
    let Some(value) = value else {
        return "\u{2014}".to_string();
    };

    let negative = value < 0;
    let digits = value.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    if negative {
        format!("-{out}")
    } else {
        out
    }
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_ascii_lowercase().contains(&needle.to_ascii_lowercase())
}

fn replace_ci(value: &str, from: &str, to: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let from_lower = from.to_ascii_lowercase();
    let mut out = String::with_capacity(value.len());
    let mut index = 0;
    while let Some(found) = lower[index..].find(&from_lower) {
        let start = index + found;
        out.push_str(&value[index..start]);
        out.push_str(to);
        index = start + from.len();
    }
    out.push_str(&value[index..]);
    out
}

/// Turns the `\uXXXX` and `\/` escapes YouTube puts in titles back into text.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('u') => {
                let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                if let Ok(code) = u32::from_str_radix(&hex, 16) {
                    if let Some(c) = char::from_u32(code) {
                        out.push(c);
                    }
                }
            }
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('/') => out.push('/'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_counts_like_the_overlay_shows_them() {
        assert_eq!(parse_count("1.234.567 views"), Some(1_234_567));
        assert_eq!(parse_count("1,234,567 views"), Some(1_234_567));
        assert_eq!(parse_count("12.3K subscribers"), Some(12_300));
        assert_eq!(parse_count("4.5 million"), Some(4_500_000));
        assert_eq!(parse_count("1.2B"), Some(1_200_000_000));
        assert_eq!(parse_count("abc"), None);
    }

    #[test]
    fn formats_in_dutch_grouping() {
        assert_eq!(fmt(Some(1_234_567)), "1.234.567");
        assert_eq!(fmt(Some(999)), "999");
        assert_eq!(fmt(None), "\u{2014}");
    }

    #[test]
    fn extracts_video_ids_and_ignores_junk() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=vOCHvWdeWTE").as_deref(),
            Some("vOCHvWdeWTE")
        );
        assert_eq!(
            extract_video_id("https://youtu.be/vOCHvWdeWTE?t=4").as_deref(),
            Some("vOCHvWdeWTE")
        );
        assert_eq!(extract_video_id("vOCHvWdeWTE").as_deref(), Some("vOCHvWdeWTE"));
        assert_eq!(extract_video_id("not a video"), None);
    }

    #[test]
    fn reads_subscriber_text_shapes() {
        let mut stats = ChannelStats::default();
        try_parse_subscribers(r#"{"subscriberCountText":"12.3K subscribers"}"#, &mut stats, false);
        assert_eq!(stats.subscribers, Some(12_300));
        assert!(stats.subs_approx);
    }
}
