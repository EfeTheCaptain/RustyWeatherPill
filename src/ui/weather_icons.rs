//! Embedded tray icons keyed by WMO weather code.
//!
//! The `.ico` files in `assets/icons/` are compiled into the exe with
//! `include_bytes!`, so the tray icon needs no files on disk at runtime.
//! Replace the placeholder art in place — no code changes required.

use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, HICON, LR_DEFAULTCOLOR,
};

/// The six tray-icon conditions, matching the WMO buckets used for the pill emoji.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WeatherIcon {
    Sunny,
    Cloudy,
    Rain,
    Snow,
    Storm,
    Fog,
}

impl WeatherIcon {
    /// Embedded .ico bytes (whole file: ICONDIR + one image).
    pub fn bytes(self) -> &'static [u8] {
        match self {
            WeatherIcon::Sunny => include_bytes!("../../assets/icons/sunny.ico"),
            WeatherIcon::Cloudy => include_bytes!("../../assets/icons/cloudy.ico"),
            WeatherIcon::Rain => include_bytes!("../../assets/icons/rain.ico"),
            WeatherIcon::Snow => include_bytes!("../../assets/icons/snow.ico"),
            WeatherIcon::Storm => include_bytes!("../../assets/icons/storm.ico"),
            WeatherIcon::Fog => include_bytes!("../../assets/icons/fog.ico"),
        }
    }
}

/// WMO weather code -> tray icon. `None` for non-weather states (e.g. the -1
/// error sentinel), where the current icon must be left untouched.
pub fn for_code(code: i32) -> Option<WeatherIcon> {
    if code < 0 {
        return None;
    }
    Some(match code {
        0 => WeatherIcon::Sunny,
        1..=3 => WeatherIcon::Cloudy,
        45 | 48 => WeatherIcon::Fog,
        51..=67 => WeatherIcon::Rain,
        71..=77 => WeatherIcon::Snow,
        80..=82 => WeatherIcon::Rain,
        85 | 86 => WeatherIcon::Snow,
        95..=99 => WeatherIcon::Storm,
        _ => WeatherIcon::Cloudy,
    })
}

/// Build a live `HICON` from an embedded .ico file.
///
/// `CreateIconFromResourceEx` wants the raw icon image (BITMAPINFOHEADER +
/// colour bitmap + AND mask), i.e. the bytes after the 6-byte ICONDIR and
/// 16-byte ICONDIRENTRY header, so the directory is parsed here.
pub fn load(icon: WeatherIcon) -> Option<HICON> {
    let data = icon.bytes();
    if data.len() < 22 {
        return None;
    }
    let count = u16::from_le_bytes([data[4], data[5]]);
    if count == 0 {
        return None;
    }
    let entry = 6; // first ICONDIRENTRY
    let size = u32::from_le_bytes(data[entry + 8..entry + 12].try_into().ok()?) as usize;
    let offset = u32::from_le_bytes(data[entry + 12..entry + 16].try_into().ok()?) as usize;
    if size == 0 || offset.checked_add(size)? > data.len() {
        return None;
    }
    unsafe {
        CreateIconFromResourceEx(&data[offset..offset + size], true, 0x0003_0000, 0, 0, LR_DEFAULTCOLOR).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;

    #[test]
    fn wmo_buckets_match_the_emoji_mapping() {
        assert_eq!(for_code(0), Some(WeatherIcon::Sunny));
        assert_eq!(for_code(1), Some(WeatherIcon::Cloudy));
        assert_eq!(for_code(3), Some(WeatherIcon::Cloudy));
        assert_eq!(for_code(45), Some(WeatherIcon::Fog));
        assert_eq!(for_code(48), Some(WeatherIcon::Fog));
        assert_eq!(for_code(51), Some(WeatherIcon::Rain));
        assert_eq!(for_code(63), Some(WeatherIcon::Rain));
        assert_eq!(for_code(67), Some(WeatherIcon::Rain));
        assert_eq!(for_code(71), Some(WeatherIcon::Snow));
        assert_eq!(for_code(77), Some(WeatherIcon::Snow));
        assert_eq!(for_code(80), Some(WeatherIcon::Rain));
        assert_eq!(for_code(82), Some(WeatherIcon::Rain));
        assert_eq!(for_code(85), Some(WeatherIcon::Snow));
        assert_eq!(for_code(86), Some(WeatherIcon::Snow));
        assert_eq!(for_code(95), Some(WeatherIcon::Storm));
        assert_eq!(for_code(99), Some(WeatherIcon::Storm));
        // gaps fall back to cloudy, like the emoji default bucket
        assert_eq!(for_code(20), Some(WeatherIcon::Cloudy));
        assert_eq!(for_code(50), Some(WeatherIcon::Cloudy));
        // error sentinel never swaps the icon
        assert_eq!(for_code(-1), None);
    }

    #[test]
    fn every_embedded_placeholder_is_a_loadable_icon() {
        for icon in [
            WeatherIcon::Sunny,
            WeatherIcon::Cloudy,
            WeatherIcon::Rain,
            WeatherIcon::Snow,
            WeatherIcon::Storm,
            WeatherIcon::Fog,
        ] {
            let h = load(icon).unwrap_or_else(|| panic!("{icon:?} failed to load"));
            assert!(!h.is_invalid());
            unsafe { let _ = DestroyIcon(h); }
        }
    }
}
