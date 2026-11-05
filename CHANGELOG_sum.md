# Changelog — RustyWeatherPill

All notable changes to the RustyWeatherPill project, from initial scaffold to current state.

---

## [Unreleased] — Current build

### Added
- Complete Rust port of C# `WeatherPill` on a taskbar shell foundation
- Taskbar pill widget with temperature + weather emoji (GDI+ double-buffered rendering)
- System tray icon with context menu (Force Update / Settings / Exit)
- Forecast popup (temperature curve + area fill, precipitation bars/%, hour labels, min/max annotations, grip resize)
- Settings dialog (city, X/Y offsets, font family/size, graph scale, UI size, theme)
- Weather service: Open-Meteo geocoding + forecast + 45-minute file cache
- Single-instance mutex guard
- Portable release build with statically-linked MSVC CRT (`make-portable.ps1`)
- Tray icon tooltip: `"{} {}°C, {}% precip"` (description, temperature, precip chance)

### Changed
- **Web source:** Weather data fetched from Open-Meteo API (`current=temperature_2m,weather_code` + `hourly=temperature_2m,precipitation_probability&forecast_days=2&timezone=auto`)
- **Tray icon:** Removed `app.ico` from tray entirely; tray starts empty and shows only a weather condition icon (sunny/cloudy/rain/snow/storm/fog) set on first weather result
- **Emoji system:** Collapsed three emoji sets (standard/minimalist/outline) into a single outline set; removed `emoji_set` from `Settings` and the settings dropdown
- **Graph scales:** Added missing `1.75` step to match C# NumericUpDown (0.25 increments): `[1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0]`
- **Emoji mapping:** Added explicit buckets for rain showers (80–82 → rain) and snow showers (85–86 → snow) so they no longer fall through to the storm/unknown catchall
- **Emoji size:** `EMOJI_FONT_RATIO` changed from `0.92` to `1.38` so default emoji is 1.5× its previous size and aligns with temperature text borders
- **Settings UI:** Added `ui_size` setting (Small/Medium/Large) that scales dialog fonts and control heights independently of pill text size
- **Settings UI:** Combo dropdown item heights now adapt to UI size (32/42/52px) via `CB_SETITEMHEIGHT`; dropdown dropped widths widened (fonts 600px, graph 220px, UI size 180px)
- **Settings UI:** Layout rewritten with grouped sections (LOCATION, WIDGET POSITION, APPEARANCE), consistent alignment at x=182, compact margins

### Fixed
- **Tray hover opening context menu:** `WM_TRAY` fired on every mouse message; now only `lparam == WM_RBUTTONUP` opens the menu
- **Context menu rendering under the taskbar:** Removed `attach_to_taskbar()` (`GWLP_HWNDPARENT = Shell_TrayWnd` dragged menus into taskbar z-band). Replaced `TPM_BOTTOMALIGN` with `TPMPARAMS.rcExclude` set to the taskbar strip, so `TrackPopupMenuEx` never places rows under the taskbar
- **Emoji / tooltip mismatch:** Rain showers (80–82) previously rendered storm emoji while tooltip said "Rain showers"; now both read from same WMO bucket (rain)
- **Pill emoji alignment:** Emoji now vertically centered and 1.5× larger relative to temperature text

### Removed
- `app.ico` from the tray icon path (tray is weather-icon only)
- Three-way emoji choice from settings dialog (`standard`/`minimalist`/`outline`)
- `attach_to_taskbar()` / `GWLP_HWNDPARENT` owner manipulation on app window
- Fixed `CLIENT_X_SAVE`, `BTN_H`, `ODT_BUTTON` unused constants

---

## Detailed Bug Fixes

### Context menu rendering under / behind the taskbar

**Symptom:** Right-clicking the pill or tray icon produced a context menu partially or fully hidden under the taskbar strip.

**Root cause (two-part):**
1. `TPM_BOTTOMALIGN` anchored the menu bottom at the cursor *inside* the taskbar strip (pill at y≈1393, taskbar top ≈1385), leaving ~25px under the taskbar.
2. `attach_to_taskbar()` set `GWLP_HWNDPARENT = Shell_TrayWnd` on the app window. Owned windows inherit the owner's z-band, as do their popups. The taskbar repaints its band on activation, painting over anything sharing it.

**Fix:** Removed `attach_to_taskbar()` entirely; app window is now unowned. Replaced `TPM_BOTTOMALIGN` with `TPMPARAMS.rcExclude` = taskbar strip (monitor `bounds` vs `work`). `TrackPopupMenuEx` refuses to place rows in the taskbar band.

**Verification:** Real `SendInput` right-clicks on pill and tray icon produced menus with bottom edge exactly at `y=1385` (taskbar top) — 0px overlap.

### Tray hover opening the context menu

**Symptom:** Moving the mouse over the tray icon opened the context menu.

**Root cause:** `WM_TRAY` carries the raw mouse message in `lParam` (classic callback semantics). The handler matched unconditionally, so `WM_MOUSEMOVE` triggered the menu.

**Fix:** Filter so menu opens only when `lparam == WM_RBUTTONUP`.

**Verification:** Hover + wiggle approach at physical tray icon → 0 menus; left-click → 0 menus; right-click → exactly 1 menu.

### Stale release binary after `cargo build --release`

**Symptom:** Deleting the exe then building reported "Finished in 0.02s" and restored the old binary.

**Root cause:** Cargo fingerprint cache considers build up-to-date when source fingerprints unchanged; deleting output alone doesn't force recompile.

**Fix:** Touch source before build to invalidate fingerprint; verify via `(Get-FileHash ...).Hash` + `LastWriteTime`. `make-portable.ps1` relies on `.cargo/config.toml` rustflags change (`+crt-static`) to force recompilation.

### Emoji / tray icon mismatch for rain showers and snow showers

**Symptom:** Pill showed storm emoji (`⛈️`) while tooltip said "Rain showers".

**Root cause:** `code_to_emoji()` had no buckets for WMO 80–82 / 85–86, fell to storm/default catchall. `weather_description()` mapped them correctly.

**Fix:** Added explicit arms: 80–82 → `☔`, 85–86 → `❄️`.

### Tray icon showing `app.ico` before first weather result

**Symptom:** Tray briefly showed generic `app.ico` on startup.

**Root cause:** `TrayIcon::new()` loaded `app.ico` via `LoadImageW(MAKEINTRESOURCE(1))` and added it immediately.

**Fix:** Removed app.ico load from `TrayIcon::new()`. Tray starts empty; first weather icon set via `sync_tray_icon()`.

### Combo dropdown clipping at tall font sizes

**Symptom:** Settings dialog at Large (22pt) clipped combo text vertically.

**Root cause:** `CBS_OWNERDRAWFIXED` uses default ~16px item height unless `CB_SETITEMHEIGHT` called; `draw_combo()` padding was `item_height / 3` (~5px).

**Fix:** Added `combo_set_item_height()` (adaptive 32/42/52px); changed `draw_combo()` padding from `item_height / 3` to fixed `6px`.

### Linker lock — `LNK1104: cannot open file`

**Symptom:** `link.exe` could not write output exe.

**Root cause:** Previous `rusty_weather_pill.exe` instance still running, holding file lock.

**Fix:** `make-portable.ps1` stops all running copies before deleting/rebuilding.

---

## Web Source Change

**Weather data source: Open-Meteo** (free, no API key required)

| Endpoint | URL |
|----------|-----|
| Geocoding | `https://geocoding-api.open-meteo.com/v1/search?name={city}&count=1` |
| Current weather | `https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,weather_code&timezone=auto` |
| Hourly forecast | `...&hourly=temperature_2m,precipitation_probability&forecast_days=2&timezone=auto` |

The WMO `weather_code` (0–99) drives tray icon bucket, pill emoji, and tooltip description — all from one `WeatherInfo` struct posted via `WM_WEATHER_RESULT`. Single source of truth.

### WMO Code → Icon / Emoji Mapping

| WMO | Condition | Tray Icon | Pill Emoji |
|-----|-----------|-----------|------------|
| 0 | Clear | sunny.ico | ☀️ |
| 1–3 | Mainly clear / partly cloudy | cloudy.ico | ⛅ |
| 45, 48 | Fog | fog.ico | ≋ |
| 51–67 | Drizzle / rain / freezing | rain.ico | ☔ |
| 71–77 | Snow | snow.ico | ❄️ |
| 80–82 | Rain showers | rain.ico | ☔ |
| 85, 86 | Snow showers | snow.ico | ❄️ |
| 95–99 | Thunderstorm | storm.ico | ⛈️ |
| other | Unknown | cloudy.ico | 🌡️ |

---

*Generated for RustyWeatherPill v0.1.0. Source: F:\AI\APPS\RustyWeatherPill\*