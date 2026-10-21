//! App preferences, saved as JSON next to the followed list
//! (e.g. %APPDATA%\PkgStats\settings.json on Windows).

use eframe::egui::Color32;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct Settings {
    /// Accent colour as RGB.
    #[serde(default = "default_accent")]
    pub accent: [u8; 3],
}

fn default_accent() -> [u8; 3] {
    let [r, g, b, _] = crate::theme::BLUE.to_array();
    [r, g, b]
}

impl Default for Settings {
    fn default() -> Self {
        Self { accent: default_accent() }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        crate::track::Store::path().with_file_name("settings.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    pub fn accent(&self) -> Color32 {
        let [r, g, b] = self.accent;
        Color32::from_rgb(r, g, b)
    }

    pub fn set_accent(&mut self, color: Color32) {
        let [r, g, b, _] = color.to_array();
        self.accent = [r, g, b];
    }
}
