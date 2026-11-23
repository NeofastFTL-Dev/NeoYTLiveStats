//! User-editable configuration, stored in the per-user config folder:
//! - Windows: `%APPDATA%\YTLiveStats\settings.json`
//! - macOS: `~/Library/Application Support/YTLiveStats/settings.json`
//! - elsewhere: `$XDG_CONFIG_HOME/ytlivestats/settings.json` (or `~/.config/...`)
//!
//! Command-line arguments override these values for the current session only.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::yt;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Settings {
    pub handle: String,
    pub video_id: String,
    /// White flash every N new subscribers.
    pub subs_flash_step: i64,
    /// Rainbow flash every N new video views.
    pub video_views_flash_step: i64,
    /// Rainbow flash every N new channel views.
    pub channel_views_flash_step: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            handle: "NeofastFTL_YT".to_string(),
            video_id: "vOCHvWdeWTE".to_string(),
            subs_flash_step: 10,
            video_views_flash_step: 100,
            channel_views_flash_step: 100,
        }
    }
}

impl Settings {
    pub fn normalized(mut self) -> Self {
        let handle = self.handle.trim().trim_start_matches('@').trim().to_string();
        self.handle = if handle.is_empty() {
            "NeofastFTL_YT".to_string()
        } else {
            handle
        };

        if self.video_id.trim().is_empty() {
            self.video_id = "vOCHvWdeWTE".to_string();
        }
        self.video_id = yt::extract_video_id(&self.video_id).unwrap_or_else(|| self.video_id.trim().to_string());

        if self.subs_flash_step <= 0 {
            self.subs_flash_step = 10;
        }
        if self.video_views_flash_step <= 0 {
            self.video_views_flash_step = 100;
        }
        if self.channel_views_flash_step <= 0 {
            self.channel_views_flash_step = 100;
        }
        self
    }
}

/// Folder for settings, the source health log and the proof dump.
pub fn data_dir() -> PathBuf {
    if cfg!(windows) {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.trim().is_empty() {
                return PathBuf::from(appdata).join("YTLiveStats");
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            if !home.trim().is_empty() {
                return PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("YTLiveStats");
            }
        }
    } else if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("ytlivestats");
        }
    } else if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home).join(".config").join("ytlivestats");
        }
    }

    std::env::temp_dir().join("ytlivestats")
}

pub fn file_path() -> PathBuf {
    data_dir().join("settings.json")
}

/// The app kept its files in a `NeoYTLiveStats` folder before it was renamed. Move that
/// folder across on the first start afterwards, so the settings, the source health log and
/// the saved counters are still there.
pub fn migrate_legacy_folder() {
    let dir = data_dir();
    if dir.exists() {
        return;
    }

    let legacy = if cfg!(windows) {
        std::env::var("APPDATA")
            .ok()
            .map(|a| PathBuf::from(a).join("NeoYTLiveStats"))
    } else {
        dir.parent().map(|p| p.join("ytlive-stats-legacy"))
    };

    if let Some(legacy) = legacy {
        if legacy.exists() {
            let _ = fs::rename(legacy, dir);
        }
    }
}

pub fn load() -> Settings {
    // Saved settings first, then command-line arguments override them for this run.
    migrate_legacy_folder();

    match fs::read_to_string(file_path()) {
        Ok(json) => serde_json::from_str::<Settings>(&json)
            .map(|s| s.normalized())
            .unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

impl Settings {
    /// Saving is best effort; the running window keeps the new values either way.
    pub fn save(&self) {
        let path = file_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }
}
