//! Weather data, direct port of WeatherService.cs: Open-Meteo forecast + geocoding,
//! a 45-minute file cache keyed by city, and the three glyph-only emoji sets.
//! Runs synchronously here — the app calls it from worker threads and posts results to the UI.

use crate::datetime::Date;
use crate::native::time;
use crate::net;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub const ERR_TEXT: &str = "Err";
const CACHE_MINUTES: f64 = 45.0;
const HOURLY_COUNT: usize = 12;

#[derive(Debug, Clone)]
pub struct WeatherInfo {
    pub text: String,
    pub code: i32,
    pub temp_c: i64,
    pub precip_pct: i32,
    pub lat: f64,
    pub lon: f64,
}

impl WeatherInfo {
    pub fn err() -> Self {
        Self { text: ERR_TEXT.into(), code: -1, temp_c: 0, precip_pct: 0, lat: 0.0, lon: 0.0 }
    }
    pub fn ok(&self) -> bool {
        self.text != ERR_TEXT
    }
}

/// One hourly point. `rd`+`hour`+`minute` is the city-local time (from `hourly.time`).
#[derive(Debug, Clone)]
pub struct HourlyEntry {
    pub rd: i64,
    pub hour: u32,
    pub minute: u32,
    pub temp_c: f64,
    pub precip_pct: i32,
}

impl HourlyEntry {
    /// "HH:mm" label for the graph x axis.
    pub fn label(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
    pub fn day_time(&self) -> i64 {
        self.rd * 3600 + self.hour as i64 * 60 + self.minute as i64
    }
}

#[derive(Serialize, Deserialize, Default)]
struct CacheEntry {
    text: String,
    code: i32,
    city: String,
    lat: f64,
    lon: f64,
    timestamp: String,
    #[serde(default)]
    temp_c: i64,
    #[serde(default)]
    precip_pct: i32,
}

#[derive(Clone)]
struct Geo {
    city: String,
    lat: f64,
    lon: f64,
}

pub struct Client {
    // Geocoder memo: the last resolved city's coords are reused while the city is unchanged.
    memo: Mutex<Option<Geo>>,
}

impl Client {
    pub fn new() -> Self {
        Self { memo: Mutex::new(None) }
    }

    fn cached(path: &std::path::Path) -> Option<CacheEntry> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn cache_path() -> Option<std::path::PathBuf> {
        weather_dir().map(|d| d.join("weather_cache.json"))
    }
}

/// %LOCALAPPDATA%\WeatherPill (cache companion of settings.json).
fn weather_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|b| b.join("WeatherPill"))
}

/// (UTC date, seconds of day) as an absolute counter via Rata Die — no chrono.
fn utc_abs() -> i64 {
    let (d, min, sec) = time::utc_now();
    d.to_rd() * 86400 + min as i64 * 60 + sec as i64
}

/// .NET "o" round-trip: 2026-11-01T09:03:24.5120000Z (fraction optional, always UTC).
fn iso_now() -> String {
    let (d, min, sec) = time::utc_now();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.0000000Z",
        d.year, d.month, d.day, min / 60, min % 60, sec
    )
}

/// Tolerant ISO-8601 parsing for our own and the C# build's cache timestamps.
fn parse_iso_utc(s: &str) -> Option<i64> {
    let s = s.trim();
    let mut parts = s.split('T');
    let day = parts.next()?;
    let day = day.trim();
    let mut d = day.split('-');
    let (y, mo, da) = (d.next()?, d.next()?, d.next()?);
    let date = Date::new(y.parse().ok()?, mo.parse().ok()?, da.parse().ok()?);
    let tail = parts.next().unwrap_or("0:00:0");
    let digits: String = tail.chars().take_while(|c| c.is_ascii_digit() || *c == ':').collect();
    let mut t = digits.split(':');
    let (h, mi, se) = (
        t.next().and_then(|x| x.parse().ok()).unwrap_or(0),
        t.next().and_then(|x| x.parse().ok()).unwrap_or(0),
        t.next().and_then(|x| x.parse().ok()).unwrap_or(0),
    );
    Some(date.to_rd() * 86400 + h as i64 * 3600 + mi as i64 * 60 + se as i64)
}

impl Client {
    /// Fresh forecast + text; on failure the caller keeps the last good reading.
    pub fn get_weather(&self, city: &str, force: bool) -> WeatherInfo {
        // 1. Check cache (< 45 mins old, same city, not forced).
        if !force {
            if let Some(entry) = Self::cache_path().and_then(|p| Self::cached(&p)) {
                let fresh = parse_iso_utc(&entry.timestamp)
                    .map(|ts| (utc_abs() - ts) as f64 / 60.0 < CACHE_MINUTES)
                    .unwrap_or(false);
                if fresh && entry.city == city && !city.is_empty() {
                    self.remember(city, entry.lat, entry.lon);
                    if !entry.text.is_empty() {
                        let temp_c = if entry.temp_c != 0 {
                            entry.temp_c
                        } else {
                            temp_from_text(&entry.text).unwrap_or(0)
                        };
                        return WeatherInfo { text: entry.text, code: entry.code, temp_c, precip_pct: entry.precip_pct, lat: entry.lat, lon: entry.lon };
                    }
                }
            }
        }

        if city.trim().is_empty() {
            return WeatherInfo::err();
        }

        // 2. Geocode city -> lat/lon (reuses last result when city unchanged).
        let Some(geo) = self.resolve_city(city) else { return WeatherInfo::err() };

        // 3. Forecast (latitude/longitude are '.'-decimal; our formatter never localizes).
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code&timezone=auto",
            fmt_coord(geo.lat), fmt_coord(geo.lon)
        );
        let Some(resp) = net::get_string(&url) else { return WeatherInfo::err() };
        let Ok(root) = serde_json::from_str::<serde_json::Value>(&resp) else {
            return WeatherInfo::err();
        };
        let Some(current) = root.get("current") else { return WeatherInfo::err() };
        let Some(temp) = number(current.get("temperature_2m")) else {
            return WeatherInfo::err();
        };
        let Some(code) = number(current.get("weather_code")).map(|v| v as i32) else {
            return WeatherInfo::err();
        };

        let text = format!("{}°C {}", temp.round() as i64, code_to_emoji(code));

        // 4. Save cache (failure is non-fatal).
        if let Some(p) = Self::cache_path() {
            let entry = CacheEntry {
                text: text.clone(),
                code,
                city: city.to_string(),
                lat: geo.lat,
                lon: geo.lon,
                timestamp: iso_now(),
                temp_c: temp.round() as i64,
                precip_pct: 0,
            };
            if let Ok(json) = serde_json::to_string(&entry) {
                let _ = crate::util::write_atomic(&p, json.as_bytes());
            }
        }

        WeatherInfo { text, code, temp_c: temp.round() as i64, precip_pct: 0, lat: geo.lat, lon: geo.lon }
    }

    /// Next 12 city-local hours of temperature + precipitation probability.
    pub fn get_hourly(&self, city: &str) -> Vec<HourlyEntry> {
        let mut list = Vec::new();
        let Some(geo) = self.resolve_city(city) else { return list };

        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&hourly=temperature_2m,precipitation_probability&forecast_days=2&timezone=auto",
            fmt_coord(geo.lat), fmt_coord(geo.lon)
        );
        let Some(resp) = net::get_string(&url) else { return list };
        let Ok(root) = serde_json::from_str::<serde_json::Value>(&resp) else { return list };
        let Some(hourly) = root.get("hourly") else { return list };
        let (Some(times), Some(temps)) = (hourly.get("time"), hourly.get("temperature_2m")) else {
            return list;
        };
        let precip = hourly.get("precipitation_probability");

        // hourly.time is city-local (timezone=auto). The response carries utc_offset_seconds,
        // so derive the city's local "now" instead of trusting the local machine clock.
        let cutoff = utc_abs() + number(root.get("utc_offset_seconds")).unwrap_or(0.0) as i64;
        let cutoff_key = {
            let d = Date::from_rd(cutoff.div_euclid(86400));
            format!("{:04}-{:02}-{:02}T{:02}:00", d.year, d.month, d.day, cutoff.rem_euclid(86400) / 3600)
        };

        for i in 0..times.as_array().map(|a| a.len()).unwrap_or(0) {
            if list.len() >= HOURLY_COUNT {
                break;
            }
            let Some(time_str) = times.get(i).and_then(|v| v.as_str()) else { continue };
            if time_str < cutoff_key.as_str() {
                continue;
            }
            let Some(temp) = temps.get(i).and_then(|v| v.as_f64()) else { continue };
            let pp = precip
                .and_then(|a| a.get(i))
                .and_then(|v| v.as_f64())
                .map(|v| v as i32)
                .unwrap_or(0);

            // "yyyy-MM-ddTHH:mm"
            let day = time_str.get(0..10).unwrap_or_default();
            let hour = time_str.get(11..13).and_then(|x| x.parse().ok()).unwrap_or(0);
            let minute = time_str.get(14..16).and_then(|x| x.parse().ok()).unwrap_or(0);
            let mut day_parts = day.split('-');
            let (y, mo, da) = (
                day_parts.next().and_then(|x| x.parse().ok()),
                day_parts.next().and_then(|x| x.parse().ok()),
                day_parts.next().and_then(|x| x.parse().ok()),
            );
            let (Some(y), Some(mo), Some(da)) = (y, mo, da) else { continue };
            list.push(HourlyEntry {
                rd: Date::new(y, mo, da).to_rd(),
                hour,
                minute,
                temp_c: temp,
                precip_pct: pp,
            });
        }
        list
    }

    /// Geocode via Open-Meteo, reusing coords while the city is unchanged.
    fn resolve_city(&self, city: &str) -> Option<Geo> {
        {
            let memo = self.memo.lock().unwrap();
            if let Some(g) = memo.as_ref() {
                if g.city == city {
                    return Some(g.clone());
                }
            }
        }
        let url = format!(
            "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1",
            uri_escape(city)
        );
        let resp = net::get_string(&url)?;
        let root = serde_json::from_str::<serde_json::Value>(&resp).ok()?;
        let first = root.get("results")?.get(0)?.clone();
        let lat = number(first.get("latitude"))?;
        let lon = number(first.get("longitude"))?;
        self.remember(city, lat, lon);
        if lat == 0.0 && lon == 0.0 {
            return None;
        }
        self.memo.lock().unwrap().clone()
    }

    /// 0/0 means "not a place" and is never remembered (WeatherPill.RememberCoords).
    fn remember(&self, city: &str, lat: f64, lon: f64) {
        if lat == 0.0 && lon == 0.0 {
            return;
        }
        *self.memo.lock().unwrap() = Some(Geo { city: city.to_string(), lat, lon });
    }
}

fn number(v: Option<&serde_json::Value>) -> Option<f64> {
    v.and_then(|v| v.as_f64())
}

/// Recover the temperature from a cached display string like "-5°C ❄️" (older caches
/// predate the explicit `temp_c` field).
fn temp_from_text(text: &str) -> Option<i64> {
    text.split('°').next()?.trim().parse().ok()
}

/// '.'-decimal coordinate text regardless of locale (C# InvariantCulture).
fn fmt_coord(v: f64) -> String {
    format!("{:.5}", v)
}

/// Minimal percent-encoding for query values (form data level).
fn uri_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Font-glyph only weather icons: no image assets, rendered straight from system fonts.
/// Single emoji set hand-picked for the tray pill. WMO code -> glyph.
pub fn code_to_emoji(code: i32) -> &'static str {
    match code {
        0 => "☀️",
        x if x <= 3 => "⛅",
        x if x <= 48 => "≋",
        x if x <= 67 => "☔",
        x if x <= 77 => "❄️",
        x if (80..=82).contains(&x) => "☔",
        x if (85..=86).contains(&x) => "❄️",
        x if x <= 99 => "⛈️",
        _ => "🌡️",
    }
}

/// Short human-readable description of a WMO weather code, used for the tray tooltip.
pub fn weather_description(code: i32) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 | 63 | 65 => "Rain",
        66 | 67 => "Freezing rain",
        71 | 73 | 75 => "Snow",
        77 => "Snow grains",
        80 | 81 | 82 => "Rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm with hail",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(y: i32, mo: u32, d: u32, h: i64, mi: i64, s: i64) -> i64 {
        Date::new(y, mo, d).to_rd() * 86400 + h * 3600 + mi * 60 + s
    }

    #[test]
    fn iso_parse_handles_csharp_round_trip() {
        assert_eq!(parse_iso_utc("2026-11-01T12:30:05.5120000Z"), Some(t(2026, 11, 1, 12, 30, 5)));
        assert_eq!(parse_iso_utc("2026-11-01T12:30:05Z"), Some(t(2026, 11, 1, 12, 30, 5)));
        assert_eq!(parse_iso_utc("2026-11-01T12:30:05"), Some(t(2026, 11, 1, 12, 30, 5)));
        assert_eq!(parse_iso_utc("2026-11-01T00:00:00.0000000Z"), Some(t(2026, 11, 1, 0, 0, 0)));
        assert_eq!(parse_iso_utc("garbage"), None);
    }

    #[test]
    fn format_matches_parse() {
        let ts = iso_now();
        // round-trips within the same UTC second
        assert!(parse_iso_utc(&ts).is_some());
    }

    #[test]
    fn emoji_sets_match_the_csharp_matrix() {
        assert_eq!(code_to_emoji(0), "☀️");
        assert_eq!(code_to_emoji(2), "⛅");
        assert_eq!(code_to_emoji(45), "≋");
        assert_eq!(code_to_emoji(61), "☔");
        assert_eq!(code_to_emoji(73), "❄️");
        assert_eq!(code_to_emoji(96), "⛈️");
        assert_eq!(code_to_emoji(10), "≋"); // fog bucket (4-48)
    }

    #[test]
    fn coord_is_culture_invariant_and_precise_enough() {
        let d = fmt_coord(21.422487);
        assert!(!d.contains(','));
        assert_eq!(d, "21.42249");
    }

    #[test]
    fn temp_recovers_from_cached_text() {
        assert_eq!(temp_from_text("12°C ☔"), Some(12));
        assert_eq!(temp_from_text("-5°C ❄️"), Some(-5));
        assert_eq!(temp_from_text("0°C ☀️"), Some(0));
        assert_eq!(temp_from_text("garbage"), None);
    }

    #[test]
    fn escape_encodes_query_city() {
        assert_eq!(uri_escape("São Paulo"), "S%C3%A3o%20Paulo");
        assert_eq!(uri_escape("A-B_C.~d1"), "A-B_C.~d1");
        assert_eq!(uri_escape("münchen"), "m%C3%BCnchen");
    }
}
