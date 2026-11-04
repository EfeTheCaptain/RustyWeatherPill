//! Settings, JSON-compatible with the C# WeatherPill build
//! (%LOCALAPPDATA%\WeatherPill\settings.json, snake_case, same defaults), plus the
//! TaskbarPray-style taskbar/monitor/theme keys the Rust build adds.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Emoji-to-main font size ratio for the pill. 1.38 makes the default emoji 1.5x
/// its previous default size so it aligns with the temperature text borders.
pub const EMOJI_FONT_RATIO: f32 = 1.38;
pub const MIN_FONT_PT: f64 = 6.0;
pub const MAX_FONT_PT: f64 = 36.0;
pub const PILL_MIN_W: i32 = 40;
pub const PILL_MIN_H: i32 = 16;
pub const PAD_X: f32 = 6.0;
pub const PAD_Y: f32 = 3.0;

/// Same graph-size gate the C# uses (GraphW >= 320 && GraphH >= 190).
pub const GRAPH_MIN_W: i32 = 320;
pub const GRAPH_MIN_H: i32 = 190;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub city: String,
    /// Taskbar pill anchor: "Left" | "Right" (relative to the tray cluster edge).
    pub widget_anchor: String,
    /// Distance from the anchor edge in logical px (DPI-independent), clamped 0..2000.
    /// `offset_x` is the C# WeatherPill key — accepted as an alias for reading legacy files.
    #[serde(alias = "offset_x")]
    pub widget_offset: i32,
    /// Distance from the taskbar's top edge in logical px (WeatherPill OffsetY).
    pub offset_y: i32,
    pub font_family: String,
    pub font_size: f64, // pt, clamp 6..36
    /// Extra UI font scale 80..150 (maps TaskbarPray font_scale_pct).
    pub font_scale_pct: i32,
    /// Auto | Dark | Light | Midnight | Slate | Warm
    pub theme: String,
    /// Settings dialog size: Small | Medium | Large (does not affect the taskbar pill).
    pub ui_size: String,
    pub monitor_device_name: Option<String>,
    pub graph_w: i32,
    pub graph_h: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            city: "London".into(),
            widget_anchor: "Right".into(),
            widget_offset: 250,
            offset_y: 5,
            font_family: "Segoe UI".into(),
            font_size: 12.0,
            font_scale_pct: 100,
            theme: "Auto".into(),
            ui_size: "Medium".into(),
            monitor_device_name: None,
            graph_w: 0,
            graph_h: 0,
        }
    }
}

impl Settings {
    pub fn font_scale(&self) -> f32 {
        self.font_scale_pct.clamp(80, 150) as f32 / 100.0
    }

    pub fn font_pt(&self) -> f32 {
        self.font_size.clamp(MIN_FONT_PT, MAX_FONT_PT) as f32
    }

    /// Legacy C# persisted graph size; falls back to 480x240 when unset/too small.
    pub fn graph_size(&self) -> (i32, i32) {
        if self.graph_w >= GRAPH_MIN_W && self.graph_h >= GRAPH_MIN_H {
            (self.graph_w, self.graph_h)
        } else {
            (480, 240)
        }
    }

    /// Clamp all ranged fields; run on any settings that bypassed the form (file load).
    pub fn sanitize(&mut self) {
        if self.city.trim().is_empty() {
            self.city = "London".into();
        }
        self.font_size = self.font_size.clamp(MIN_FONT_PT, MAX_FONT_PT);
        self.font_scale_pct = self.font_scale_pct.clamp(80, 150);
        self.widget_offset = self.widget_offset.clamp(0, 2000);
        self.offset_y = self.offset_y.clamp(-1000, 1000);
        if !matches!(self.ui_size.as_str(), "Small" | "Medium" | "Large") {
            self.ui_size = "Medium".into();
        }
        if self.widget_anchor.eq_ignore_ascii_case("Left") {
            self.widget_anchor = "Left".into();
        } else {
            self.widget_anchor = "Right".into();
        }
    }

    pub fn dir() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_default();
        base.join("WeatherPill")
    }

    fn path() -> PathBuf {
        Self::dir().join("settings.json")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &std::path::Path) -> Self {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(mut s) = serde_json::from_str::<Self>(&text) {
                // Guard against the C# build's non-negative offsets plus any out-of-range values.
                s.sanitize();
                return s;
            }
        }
        Self::default() // missing or corrupt -> defaults
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = crate::util::write_atomic(&Self::path(), json.as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_restores_safe_defaults() {
        let raw = r#"{"city":" ","widget_anchor":"middle","widget_offset":9999,
            "offset_y":-9999,"font_size":99,"font_scale_pct":10,"ui_size":"huge"}"#;
        let mut s: Settings = serde_json::from_str(raw).unwrap();
        s.sanitize();
        assert_eq!(s.city, "London");
        assert_eq!(s.widget_anchor, "Right");
        assert_eq!(s.widget_offset, 2000);
        assert_eq!(s.offset_y, -1000);
        assert_eq!(s.font_size, MAX_FONT_PT);
        assert_eq!(s.font_scale_pct, 80);
        assert_eq!(s.ui_size, "Medium");
    }

    #[test]
    fn legacy_csharp_file_loads_with_new_defaults() {
        let raw = r#"{"city":"Berlin","offset_x":120,"offset_y":5,"font_family":"Consolas",
                      "font_size":12,"graph_w":480,"graph_h":240}"#;
        let mut s: Settings = serde_json::from_str(raw).unwrap();
        s.sanitize();
        assert_eq!(s.city, "Berlin");
        assert_eq!(s.font_family, "Consolas");
        // New keys fall back to defaults; legacy offset_x maps onto widget_offset.
        assert_eq!(s.widget_anchor, "Right");
        assert_eq!(s.widget_offset, 120);
        assert_eq!(s.ui_size, "Medium");
    }
}
