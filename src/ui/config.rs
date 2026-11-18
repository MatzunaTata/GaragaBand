//! User settings stored in `%APPDATA%\GaragaBand\config.json`.

use super::theme::ThemeMode;
use crate::i18n::Lang;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub(super) struct AppConfig {
    /// Output device name; `None` picks one automatically.
    pub out_dev: Option<String>,
    /// Input device name; `None` picks one automatically.
    pub in_dev: Option<String>,
    /// Try exclusive mode / small buffers first.
    pub low_latency: bool,
    /// Requested buffer period in milliseconds (low-latency mode only).
    pub period_ms: f32,
    /// Manual recording offset; `None` uses the latency reported by the device.
    pub latency_comp_ms: Option<f32>,
    /// The first-run help window has been closed once.
    pub help_shown: bool,
    /// UI language.
    pub lang: Lang,
    /// Dark or light appearance.
    pub theme: ThemeMode,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            out_dev: None,
            in_dev: None,
            low_latency: true,
            period_ms: 3.0,
            latency_comp_ms: None,
            help_shown: false,
            lang: Lang::system_default(),
            theme: ThemeMode::default(),
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("GaragaBand");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("config.json"))
}

impl AppConfig {
    /// Loads the settings; missing or broken files yield the defaults.
    pub fn load() -> Self {
        config_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    /// Saves the settings; errors are ignored because nothing depends on them.
    pub fn save(&self) {
        if let (Some(p), Ok(s)) = (config_path(), serde_json::to_string_pretty(self)) {
            let _ = std::fs::write(p, s);
        }
    }
}
