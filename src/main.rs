//! YTLiveStats — fullscreen overlay with live YouTube subscriber, channel-view and video-view
//! counters.
//!
//! Port of the WinForms app to egui/eframe (glow), so the overlay runs natively on Linux as
//! well as Windows. The polling behaviour is unchanged: two independent fetches every second,
//! each with its own fallback chain, stale numbers dim instead of disappearing, and crossing a
//! flash threshold triggers the white or rainbow flash.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod health;
mod settings;
mod yt;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{Color32, RichText};

use settings::Settings;

const BG: Color32 = Color32::from_rgb(8, 8, 12);
const PANEL: Color32 = Color32::from_rgb(16, 16, 24);
const FRESH_VID: Color32 = Color32::from_rgb(77, 166, 255);
const STALE_STAT: Color32 = Color32::from_rgb(110, 110, 130);
const STATUS_GREY: Color32 = Color32::from_rgb(90, 90, 110);
const CAPTION_GREY: Color32 = Color32::from_rgb(140, 140, 160);
const PINK: Color32 = Color32::from_rgb(255, 77, 109);
const GREEN: Color32 = Color32::from_rgb(80, 220, 120);
const GREEN_DIM: Color32 = Color32::from_rgb(40, 160, 80);
const PULSE: Color32 = Color32::from_rgb(255, 255, 120);
const RAINBOW: [Color32; 7] = [
    Color32::from_rgb(255, 60, 60),
    Color32::from_rgb(255, 160, 40),
    Color32::from_rgb(255, 230, 60),
    Color32::from_rgb(60, 220, 90),
    Color32::from_rgb(60, 160, 255),
    Color32::from_rgb(140, 80, 255),
    Color32::from_rgb(255, 80, 200),
];

/// Numbers that stopped updating dim after this many seconds, so an old value never looks fresh.
const STALE_AFTER_SECONDS: u64 = 15;

enum Msg {
    Video(yt::VideoStats),
    Channel(yt::ChannelStats),
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Stat {
    Subs,
    ChanViews,
    VidViews,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Flash {
    White,
    Rainbow,
}

struct App {
    shared: Arc<Mutex<Settings>>,
    handle: String,
    video_id: String,
    subs_step: i64,
    vid_step: i64,
    chan_step: i64,

    rx: Receiver<Msg>,

    subs: Option<i64>,
    chan_views: Option<i64>,
    vid_views: Option<i64>,
    server_chan: Option<i64>,
    vid_at_sync: Option<i64>,

    base_subs: Option<i64>,
    base_vid: Option<i64>,
    base_chan: Option<i64>,
    armed_subs: i64,
    armed_vid: i64,
    armed_chan: i64,

    last_subs: Option<i64>,
    last_chan: Option<i64>,
    last_vid: Option<i64>,
    last_vid_at: Option<Instant>,
    last_chan_at: Option<Instant>,

    poll_tick: u32,
    vid_ok: u32,
    chan_ok: u32,
    last_tick: Instant,

    chan_source: String,
    vid_source: String,
    chan_error: String,
    vid_error: String,
    status_error: bool,
    status: String,

    flash: Option<(Flash, Instant)>,
    pulse: Option<(Stat, Instant)>,

    fullscreen: bool,
    settings_open: bool,
    draft: Settings,
    test_tx: Sender<Result<String, String>>,
    test_rx: Receiver<Result<String, String>>,
    testing: bool,
    test_message: String,
    test_ok: bool,
}

impl App {
    fn new(settings: Settings, fullscreen: bool) -> Self {
        let (tx, rx) = channel();
        let (test_tx, test_rx) = channel();
        let shared = Arc::new(Mutex::new(settings.clone()));

        // Two independent pollers, like the two timers in the WinForms version: a slow channel
        // fetch never delays the video counter and the other way round.
        let channel_shared = Arc::clone(&shared);
        let channel_tx = tx.clone();
        std::thread::spawn(move || loop {
            let handle = channel_shared
                .lock()
                .map(|s| s.handle.clone())
                .unwrap_or_else(|_| "NeofastFTL_YT".to_string());
            let stats = yt::fetch_channel(&handle);
            if channel_tx.send(Msg::Channel(stats)).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        });

        let video_shared = Arc::clone(&shared);
        let video_tx = tx.clone();
        std::thread::spawn(move || loop {
            let video_id = video_shared
                .lock()
                .map(|s| s.video_id.clone())
                .unwrap_or_default();
            let stats = yt::fetch_video(&video_id);
            if video_tx.send(Msg::Video(stats)).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        });

        Self {
            handle: settings.handle.clone(),
            video_id: settings.video_id.clone(),
            subs_step: settings.subs_flash_step,
            vid_step: settings.video_views_flash_step,
            chan_step: settings.channel_views_flash_step,
            draft: settings.clone(),
            shared,
            rx,
            subs: None,
            chan_views: None,
            vid_views: None,
            server_chan: None,
            vid_at_sync: None,
            base_subs: None,
            base_vid: None,
            base_chan: None,
            armed_subs: 0,
            armed_vid: 0,
            armed_chan: 0,
            last_subs: None,
            last_chan: None,
            last_vid: None,
            last_vid_at: None,
            last_chan_at: None,
            poll_tick: 0,
            vid_ok: 0,
            chan_ok: 0,
            last_tick: Instant::now(),
            chan_source: String::new(),
            vid_source: String::new(),
            chan_error: String::new(),
            vid_error: String::new(),
            status_error: false,
            status: "Laden\u{2026}".to_string(),
            flash: None,
            pulse: None,
            fullscreen,
            settings_open: false,
            test_tx,
            test_rx,
            testing: false,
            test_message: String::new(),
            test_ok: true,
        }
    }

    fn drain_messages(&mut self) {
        let mut messages = Vec::new();
        while let Ok(message) = self.rx.try_recv() {
            messages.push(message);
        }

        for message in messages {
            match message {
                Msg::Video(stats) => self.apply_video(stats),
                Msg::Channel(stats) => self.apply_channel(stats),
            }
        }
    }

    fn apply_video(&mut self, vid: yt::VideoStats) {
        // A response for the previous video can still be in flight right after a settings
        // change; applying it would mix two targets' numbers on screen.
        if !vid.video_id.eq_ignore_ascii_case(&self.video_id) {
            return;
        }

        self.vid_source = vid.source.clone();
        self.vid_error = vid.error.clone();

        if let Some(views) = vid.views {
            self.vid_ok += 1;
            self.last_vid_at = Some(Instant::now());

            let up = self.last_vid.map(|previous| views > previous).unwrap_or(false);
            if up {
                self.pulse = Some((Stat::VidViews, Instant::now()));
            }
            if self.last_vid != Some(views) {
                self.track_vid_views(views);
            }
            self.last_vid = Some(views);
            self.vid_views = Some(views);
            self.refresh_channel_views_from_video(up);
            self.status_error = false;
        } else {
            // Keep the last known number instead of blanking it; it dims once it goes stale.
            self.status = format!(
                "Video: {}",
                if vid.error.is_empty() {
                    "geen views in het antwoord"
                } else {
                    &vid.error
                }
            );
            self.status_error = true;
        }

        self.write_proof();
    }

    fn apply_channel(&mut self, ch: yt::ChannelStats) {
        if !ch.handle.eq_ignore_ascii_case(&self.handle) {
            return;
        }

        self.chan_source = ch.source.clone();
        self.chan_error = ch.error.clone();

        if let Some(subscribers) = ch.subscribers {
            self.chan_ok += 1;
            self.last_chan_at = Some(Instant::now());

            let up = self.last_subs.map(|previous| subscribers > previous).unwrap_or(false);
            if up {
                self.pulse = Some((Stat::Subs, Instant::now()));
            }
            if self.last_subs != Some(subscribers) {
                self.track_subs(subscribers);
            }
            self.last_subs = Some(subscribers);
            self.subs = Some(subscribers);
        }

        if let Some(views) = ch.views {
            // Channel views come from the about page only once in a while, so they are synced
            // to a baseline and then advanced by video views.
            let first_sync = self.server_chan.is_none();
            let server_moved = self.server_chan.map(|previous| views > previous).unwrap_or(false);
            if first_sync || server_moved {
                self.server_chan = Some(views);
                self.vid_at_sync = self.last_vid;
            }
            self.refresh_channel_views_from_video(false);
        }

        if !ch.error.is_empty() {
            self.status = ch.error.clone();
            self.status_error = true;
        } else if ch.subscribers.is_some() {
            self.status_error = false;
        } else if self.last_subs.is_none() && self.server_chan.is_none() {
            self.status = "Kanaalcijfers ontbreken".to_string();
            self.status_error = true;
        }

        self.write_proof();
    }

    fn refresh_channel_views_from_video(&mut self, pulse_if_up: bool) {
        let Some(server) = self.server_chan else {
            return;
        };

        let delta = match (self.vid_at_sync, self.last_vid) {
            (Some(sync), Some(current)) if current > sync => current - sync,
            _ => 0,
        };

        let total = server + delta;
        let up = self.last_chan.map(|previous| total > previous).unwrap_or(false);
        if up || (pulse_if_up && delta > 0) {
            self.pulse = Some((Stat::ChanViews, Instant::now()));
        }
        if self.last_chan != Some(total) {
            self.track_chan_views(total);
        }
        self.last_chan = Some(total);
        self.chan_views = Some(total);
    }

    fn track_subs(&mut self, current: i64) {
        let Some(baseline) = self.base_subs else {
            self.base_subs = Some(current);
            self.armed_subs = self.subs_step;
            return;
        };
        while current - baseline >= self.armed_subs {
            self.trigger_white_flash();
            self.armed_subs += self.subs_step;
        }
    }

    fn track_vid_views(&mut self, current: i64) {
        let Some(baseline) = self.base_vid else {
            self.base_vid = Some(current);
            self.armed_vid = self.vid_step;
            return;
        };
        while current - baseline >= self.armed_vid {
            self.trigger_rainbow();
            self.armed_vid += self.vid_step;
        }
    }

    fn track_chan_views(&mut self, current: i64) {
        let Some(baseline) = self.base_chan else {
            self.base_chan = Some(current);
            self.armed_chan = self.chan_step;
            return;
        };
        while current - baseline >= self.armed_chan {
            self.trigger_rainbow();
            self.armed_chan += self.chan_step;
        }
    }

    fn trigger_white_flash(&mut self) {
        if !matches!(self.flash, Some((Flash::Rainbow, _))) {
            self.flash = Some((Flash::White, Instant::now()));
        }
    }

    fn trigger_rainbow(&mut self) {
        self.flash = Some((Flash::Rainbow, Instant::now()));
    }

    fn reset_tracking(&mut self) {
        self.base_subs = None;
        self.base_vid = None;
        self.base_chan = None;
        self.armed_subs = 0;
        self.armed_vid = 0;
        self.armed_chan = 0;
        self.last_subs = None;
        self.last_chan = None;
        self.last_vid = None;
        self.subs = None;
        self.chan_views = None;
        self.vid_views = None;
        self.server_chan = None;
        self.vid_at_sync = None;
    }

    fn is_stale(at: Option<Instant>) -> bool {
        match at {
            None => true,
            Some(at) => at.elapsed() > Duration::from_secs(STALE_AFTER_SECONDS),
        }
    }

    fn age_seconds(at: Option<Instant>) -> i64 {
        match at {
            None => -1,
            Some(at) => at.elapsed().as_secs() as i64,
        }
    }

    fn short_source(source: &str) -> String {
        if source.is_empty() {
            "-".to_string()
        } else {
            source.replace("innertube-", "it-")
        }
    }

    fn live_text(&self) -> String {
        let vid_age = Self::age_seconds(self.last_vid_at);
        let chan_age = Self::age_seconds(self.last_chan_at);
        let stale = Self::is_stale(self.last_vid_at) || Self::is_stale(self.last_chan_at);
        format!(
            "\u{25cf} LIVE  {}  v{}s c{}s{}",
            self.poll_tick,
            vid_age,
            chan_age,
            if stale { "  \u{00b7} stale" } else { "" }
        )
    }

    fn status_text(&self) -> String {
        if self.status_error {
            return self.status.clone();
        }
        format!(
            "@{} | {} | src {}/{} | ok v{}/c{} | 1s",
            self.handle,
            self.video_id,
            Self::short_source(&self.chan_source),
            Self::short_source(&self.vid_source),
            self.vid_ok,
            self.chan_ok
        )
    }

    fn flash_phase(&self) -> Option<(Color32, Color32)> {
        let (kind, start) = self.flash?;
        let elapsed = start.elapsed().as_millis();

        match kind {
            Flash::White => {
                if elapsed >= 1_000 {
                    None
                } else if (elapsed / 100) % 2 == 0 {
                    Some((Color32::WHITE, Color32::BLACK))
                } else {
                    Some((BG, Color32::WHITE))
                }
            }
            Flash::Rainbow => {
                if elapsed >= 2_000 {
                    None
                } else {
                    Some((RAINBOW[((elapsed / 285) as usize) % RAINBOW.len()], Color32::WHITE))
                }
            }
        }
    }

    fn stat_color(&self, stat: Stat, phase: Option<(Color32, Color32)>) -> Color32 {
        if let Some((_, foreground)) = phase {
            return foreground;
        }

        if let Some((pulsed, start)) = self.pulse {
            if pulsed == stat && start.elapsed() < Duration::from_millis(600) {
                return PULSE;
            }
        }

        let (value, at, fresh) = match stat {
            Stat::Subs => (self.subs, self.last_chan_at, Color32::WHITE),
            Stat::ChanViews => (self.chan_views, self.last_chan_at, Color32::WHITE),
            Stat::VidViews => (self.vid_views, self.last_vid_at, FRESH_VID),
        };
        match value {
            _ if Self::is_stale(at) => STALE_STAT,
            _ => fresh,
        }
    }

    fn open_settings(&mut self) {
        self.draft = Settings {
            handle: self.handle.clone(),
            video_id: self.video_id.clone(),
            subs_flash_step: self.subs_step,
            video_views_flash_step: self.vid_step,
            channel_views_flash_step: self.chan_step,
        };
        self.test_message.clear();
        self.testing = false;
        self.settings_open = true;
    }

    fn apply_settings(&mut self) {
        let handle = self.draft.handle.trim().trim_start_matches('@').trim().to_string();
        if handle.is_empty() {
            self.test_ok = false;
            self.test_message = "Vul een kanaalhandle in.".to_string();
            return;
        }

        let Some(video_id) = yt::extract_video_id(self.draft.video_id.trim()) else {
            self.test_ok = false;
            self.test_message = "Geen geldige video-id of link.".to_string();
            return;
        };

        let normalized = Settings {
            handle,
            video_id,
            subs_flash_step: self.draft.subs_flash_step.max(1),
            video_views_flash_step: self.draft.video_views_flash_step.max(1),
            channel_views_flash_step: self.draft.channel_views_flash_step.max(1),
        }
        .normalized();

        let targets_changed = !self.handle.eq_ignore_ascii_case(&normalized.handle)
            || self.video_id != normalized.video_id;

        self.handle = normalized.handle.clone();
        self.video_id = normalized.video_id.clone();
        self.subs_step = normalized.subs_flash_step;
        self.vid_step = normalized.video_views_flash_step;
        self.chan_step = normalized.channel_views_flash_step;

        if let Ok(mut shared) = self.shared.lock() {
            *shared = normalized.clone();
        }
        normalized.save();

        if targets_changed {
            self.reset_tracking();
            self.status_error = false;
            self.chan_source.clear();
            self.vid_source.clear();
            self.chan_error.clear();
            self.vid_error.clear();
            self.status = "Laden\u{2026}".to_string();
            self.last_vid_at = None;
            self.last_chan_at = None;
        }

        self.settings_open = false;
    }

    fn start_test(&mut self) {
        let handle = self.draft.handle.trim().trim_start_matches('@').trim().to_string();
        if handle.is_empty() {
            self.test_ok = false;
            self.test_message = "Vul een kanaalhandle in.".to_string();
            return;
        }
        let Some(video_id) = yt::extract_video_id(self.draft.video_id.trim()) else {
            self.test_ok = false;
            self.test_message = "Geen geldige video-id of link.".to_string();
            return;
        };

        self.testing = true;
        self.test_ok = true;
        self.test_message = "Bronnen testen\u{2026}".to_string();

        let sender = self.test_tx.clone();
        std::thread::spawn(move || {
            let channel = yt::fetch_channel(&handle);
            let video = yt::fetch_video(&video_id);

            let channel_text = format!(
                "Kanaal via {}: {}{}{}",
                if channel.source.is_empty() { "geen bron" } else { &channel.source },
                match channel.subscribers {
                    Some(value) => format!("{} subs", yt::fmt(Some(value))),
                    None => "geen subs".to_string(),
                },
                match channel.views {
                    Some(value) => format!(" \u{00b7} {} kanaal views", yt::fmt(Some(value))),
                    None => String::new(),
                },
                if channel.error.is_empty() {
                    String::new()
                } else {
                    format!(" \u{2014} {}", channel.error)
                }
            );
            let video_text = format!(
                "Video via {}: {}{}",
                if video.source.is_empty() { "geen bron" } else { &video.source },
                match video.views {
                    Some(value) => format!("{} views", yt::fmt(Some(value))),
                    None => "geen views".to_string(),
                },
                if video.error.is_empty() {
                    String::new()
                } else {
                    format!(" \u{2014} {}", video.error)
                }
            );

            let _ = sender.send(Ok(format!("{channel_text}\n{video_text}")));
        });
    }

    fn poll_test_result(&mut self) {
        while let Ok(result) = self.test_rx.try_recv() {
            self.testing = false;
            match result {
                Ok(text) => {
                    self.test_ok = true;
                    self.test_message = text;
                }
                Err(error) => {
                    self.test_ok = false;
                    self.test_message = format!("Test mislukt: {error}");
                }
            }
        }
    }

    /// Dumps the current numbers to the temp folder for inspection, like the WinForms version.
    fn write_proof(&self) {
        let payload = serde_json::json!({
            "pollTick": self.poll_tick,
            "vidOk": self.vid_ok,
            "chanOk": self.chan_ok,
            "subs": self.last_subs,
            "channelViews": self.last_chan,
            "vidViews": self.last_vid,
            "chanSource": self.chan_source,
            "vidSource": self.vid_source,
            "chanError": self.chan_error,
            "vidError": self.vid_error,
            "stale": Self::is_stale(self.last_vid_at) || Self::is_stale(self.last_chan_at),
            "serverChan": self.server_chan,
            "vidAtSync": self.vid_at_sync,
        });

        if let Ok(text) = serde_json::to_string_pretty(&payload) {
            let _ = std::fs::write(std::env::temp_dir().join("ytl_last.json"), text);
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let (escape, f11, f2) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Escape),
                input.key_pressed(egui::Key::F11),
                input.key_pressed(egui::Key::F2),
            )
        });

        if escape && self.fullscreen {
            self.set_fullscreen(ctx, false);
        }
        if f11 {
            let target = !self.fullscreen;
            self.set_fullscreen(ctx, target);
        }
        if f2 {
            self.open_settings();
        }
    }

    fn set_fullscreen(&mut self, ctx: &egui::Context, on: bool) {
        self.fullscreen = on;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
    }

    fn draw(&mut self, ctx: &egui::Context) {
        self.poll_test_result();
        let phase = self.flash_phase();
        if phase.is_none() && self.flash.is_some() {
            self.flash = None;
        }

        let background = phase.map(|(bg, _)| bg).unwrap_or(BG);
        let panel = if phase.is_some() { background } else { PANEL };

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(background).inner_margin(egui::Margin::same(28)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("@{}  \u{00b7}  Can-Caneurysm", self.handle))
                            .size(16.0)
                            .strong()
                            .color(if phase.is_some() { Color32::WHITE } else { PINK }),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let live_color = if self.poll_tick % 2 == 0 { GREEN } else { GREEN_DIM };
                        ui.label(RichText::new(self.live_text()).size(12.0).strong().color(live_color));

                        if ui
                            .button(RichText::new("\u{2699}").size(18.0))
                            .on_hover_text("Instellingen (F2)")
                            .clicked()
                        {
                            self.open_settings();
                        }
                    });
                });

                ui.add_space(10.0);

                let height = (ui.available_height() - 34.0).max(180.0);
                let width = (ui.available_width() - 24.0) / 3.0;

                ui.horizontal(|ui| {
                    let columns = [
                        ("SUBS", self.subs, Stat::Subs),
                        ("KANAAL VIEWS", self.chan_views, Stat::ChanViews),
                        ("VIDEO VIEWS", self.vid_views, Stat::VidViews),
                    ];

                    for (caption, value, stat) in columns {
                        let color = self.stat_color(stat, phase);
                        egui::Frame::NONE
                            .fill(panel)
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_width(width);
                                ui.set_height(height);
                                ui.vertical_centered(|ui| {
                                    ui.add_space(6.0);
                                    ui.label(
                                        RichText::new(caption)
                                            .size(13.0)
                                            .strong()
                                            .color(if phase.is_some() { Color32::WHITE } else { CAPTION_GREY }),
                                    );
                                    ui.add_space(((height - 120.0) / 2.0).max(0.0));
                                    ui.label(RichText::new(yt::fmt(value)).size(56.0).strong().color(color));
                                });
                            });
                        ui.add_space(12.0);
                    }
                });

                ui.add_space(4.0);
                ui.label(
                    RichText::new(self.status_text())
                        .size(10.0)
                        .color(if phase.is_some() { Color32::WHITE } else { STATUS_GREY }),
                );
            });

        if self.settings_open {
            self.draw_settings(ctx);
        }
    }

    fn draw_settings(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;

        egui::Window::new("Instellingen")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(520.0);

                egui::Grid::new("settings-grid")
                    .num_columns(2)
                    .spacing([16.0, 12.0])
                    .show(ui, |ui| {
                        ui.label("Kanaalhandle (@naam)");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft.handle)
                                .desired_width(300.0)
                                .hint_text("@NeofastFTL_YT"),
                        );
                        ui.end_row();

                        ui.label("Video-id of link");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft.video_id)
                                .desired_width(300.0)
                                .hint_text("https://youtu.be/..."),
                        );
                        ui.end_row();

                        ui.label("Wit-flits per N subs");
                        ui.add(egui::DragValue::new(&mut self.draft.subs_flash_step).range(1..=1_000_000));
                        ui.end_row();

                        ui.label("Regenboog per N video views");
                        ui.add(egui::DragValue::new(&mut self.draft.video_views_flash_step).range(1..=1_000_000));
                        ui.end_row();

                        ui.label("Regenboog per N kanaal views");
                        ui.add(egui::DragValue::new(&mut self.draft.channel_views_flash_step).range(1..=1_000_000));
                        ui.end_row();
                    });

                if !self.test_message.is_empty() {
                    ui.add_space(6.0);
                    let color = if self.test_ok { Color32::from_rgb(160, 220, 180) } else { Color32::from_rgb(255, 120, 130) };
                    ui.label(RichText::new(&self.test_message).size(11.0).color(color));
                }

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.testing, egui::Button::new("Test")).clicked() {
                        self.start_test();
                    }
                    if ui.button("Opslaan").clicked() {
                        self.apply_settings();
                    }
                    if ui.button("Annuleren").clicked() {
                        self.settings_open = false;
                    }
                });
            });

        self.settings_open = open;
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.last_tick.elapsed() >= Duration::from_secs(1) {
            self.poll_tick += 1;
            self.last_tick = Instant::now();
        }

        self.drain_messages();
        self.handle_keys(ctx);
        self.draw(ctx);

        // 20 fps while nothing changes, matching the 50 ms animation timer of the WinForms app.
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

/// Headless checks for CI: the parsers, the formatter and the settings normalisation, without
/// opening a window.
fn selftest() -> i32 {
    let mut failures = 0;
    let mut check = |name: &str, ok: bool| {
        println!("{} {name}", if ok { "\u{2714}" } else { "\u{2716}" });
        if !ok {
            failures += 1;
        }
    };

    check("parse_count Dutch grouping", yt::parse_count("1.234.567 weergaven") == Some(1_234_567));
    check("parse_count English grouping", yt::parse_count("1,234,567 views") == Some(1_234_567));
    check("parse_count K suffix", yt::parse_count("12.3K subscribers") == Some(12_300));
    check("parse_count million word", yt::parse_count("4.5 million") == Some(4_500_000));
    check("parse_count junk", yt::parse_count("abc").is_none());
    check("fmt Dutch", yt::fmt(Some(1_234_567)) == "1.234.567");
    check("fmt empty", yt::fmt(None) == "\u{2014}");
    check(
        "extract link",
        yt::extract_video_id("https://www.youtube.com/watch?v=vOCHvWdeWTE").as_deref() == Some("vOCHvWdeWTE"),
    );
    check("extract bare id", yt::extract_video_id("vOCHvWdeWTE").as_deref() == Some("vOCHvWdeWTE"));
    check("extract junk", yt::extract_video_id("not a video").is_none());

    let normalized = Settings {
        handle: "@  ".to_string(),
        video_id: String::new(),
        subs_flash_step: 0,
        video_views_flash_step: 0,
        channel_views_flash_step: 0,
    }
    .normalized();
    check("settings default handle", normalized.handle == "NeofastFTL_YT");
    check("settings default video", normalized.video_id == "vOCHvWdeWTE");
    check("settings positive steps", normalized.subs_flash_step > 0 && normalized.video_views_flash_step > 0);
    check("data dir resolvable", !settings::data_dir().as_os_str().is_empty());

    println!(
        "{}",
        if failures == 0 {
            format!("\u{2714} alles goed: {} controle(s) geslaagd", 15)
        } else {
            format!("\u{2716} {failures} van 15 controle(s) mislukt")
        }
    );

    failures
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--selftest") {
        std::process::exit(selftest());
    }

    let mut settings = settings::load();
    let mut fullscreen = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--fullscreen" | "-f" => fullscreen = true,
            "--handle" | "-h" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    settings.handle = value.clone();
                }
            }
            "--video" | "-v" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    settings.video_id = value.clone();
                }
            }
            _ => {}
        }
        index += 1;
    }

    let settings = settings.normalized();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("YTLiveStats")
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([900.0, 500.0])
            .with_fullscreen(fullscreen),
        ..Default::default()
    };

    eframe::run_native(
        "YTLiveStats",
        native_options,
        Box::new(move |_cc| Ok(Box::new(App::new(settings, fullscreen)))),
    )
}
