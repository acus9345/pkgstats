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
    /// Order of the sidebar menu items, as arranged by dragging.
    #[serde(default = "default_menu")]
    pub menu: Vec<MenuItem>,
}

/// The fixed entries at the top of the sidebar.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum MenuItem {
    Search,
    Following,
    History,
    Token,
    Settings,
}

impl MenuItem {
    pub const ALL: [MenuItem; 5] = [Self::Search, Self::Following, Self::History, Self::Token, Self::Settings];
}

fn default_menu() -> Vec<MenuItem> {
    MenuItem::ALL.to_vec()
}

fn default_accent() -> [u8; 3] {
    let [r, g, b, _] = crate::theme::CLAUDE.to_array();
    [r, g, b]
}

impl Default for Settings {
    fn default() -> Self {
        Self { accent: default_accent(), menu: default_menu() }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        crate::track::Store::path().with_file_name("settings.json")
    }

    pub fn load() -> Self {
        let mut settings: Self = std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        settings.menu = complete_menu(&settings.menu);
        settings
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

    pub fn move_menu_item(&mut self, from: usize, to: usize) {
        crate::move_item(&mut self.menu, from, to);
    }

    pub fn set_accent(&mut self, color: Color32) {
        let [r, g, b, _] = color.to_array();
        self.accent = [r, g, b];
    }
}

/// Keeps a saved order but drops duplicates and adds items it's missing, so a
/// hand-edited or older settings file still shows every menu item once.
fn complete_menu(saved: &[MenuItem]) -> Vec<MenuItem> {
    let mut menu: Vec<MenuItem> = Vec::new();
    for item in saved.iter().chain(MenuItem::ALL.iter()) {
        if !menu.contains(item) {
            menu.push(*item);
        }
    }
    menu
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_menu_order_is_kept_and_completed() {
        use MenuItem::*;
        assert_eq!(complete_menu(&[History, Search, History]), vec![History, Search, Following, Token, Settings]);
        assert_eq!(complete_menu(&[]), MenuItem::ALL.to_vec());
    }
}
