# RustyWeatherPill — Notes & Documentation

**Package:** `rusty_weather_pill` **v0.1.3**  
**License:** MIT  
**Location:** `F:\AI\APPS\RustyWeatherPill\`  
**Description:** Rust port of the C# `WeatherPill` taskbar weather app on a taskbar shell foundation. A standalone, portable Windows 11 tray widget that shows current weather in the taskbar and a forecast popup.

---

## Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [File Layout](#file-layout)
4. [Build & Portable Distribution](#build--portable-distribution)
5. [Running](#running)
6. [Configuration](#configuration)
7. [UI Components](#ui-components)
8. [Weather Service](#weather-service)
9. [Testing](#testing)
10. [Porting Notes (C# → Rust)](#porting-notes-c--rust)
11. [Changelog](#changelog)
12. [Troubleshooting & Difficult Problems](#troubleshooting--difficult-problems)

---

## Overview

RustyWeatherPill is a native Windows taskbar weather pill written in Rust using the `windows` crate (Win32 API FFI) and GDI+ for rendering. It:

- Lives as a pill widget over the Windows taskbar strip
- Shows current temperature + weather condition emoji
- Updates the system tray icon to match the weather condition
- Opens a forecast popup (temperature curve + precipitation bars) on pill left-click
- Opens a context menu (Force Update / Clear Cache / Settings / Exit) on pill right-click
- Runs as a single instance (mutex-guarded)
- Stores settings and cache in `%LOCALAPPDATA%\WeatherPill\`
- Is distributed as a portable, single-exe release with a statically linked MSVC CRT (no Visual C++ Redistributable required)

See `EFFICIENCY.md` for the plain-language explanation of storage, update cadence, and footprint.

---

## Architecture

```
rusty_weather_pill.exe
├── main.rs                          # Entry point; single-instance mutex
├── lib.rs                           # Module root
├── app.rs                           # App orchestrator (window, tray, widget, weather, forecast, settings)
├── config.rs                        # Settings (JSON, %LOCALAPPDATA%\WeatherPill\settings.json)
├── weather.rs                       # WeatherClient: Open-Meteo API, geocoding, forecast, WMO codes, emoji
├── net.rs                           # HTTP client (WinHttp)
├── datetime.rs                      # Date utilities (Rata Die)
├── util.rs                          # Atomic file writes
├── ui/
│   ├── window.rs                    # HWND creation wrapper
│   ├── widget.rs                    # Taskbar pill widget (rendering, drag, hover, fullscreen hide)
│   ├── icon.rs                      # Tray icon (Shell_NotifyIcon, tooltip, icon swap)
│   ├── weather_icons.rs             # Embedded .ico assets keyed to WMO codes
│   ├── forecast.rs                  # Forecast popup (GDI+ graph, precip bars)
│   ├── settings.rs                  # Settings dialog (Win32 controls)
│   ├── theme.rs                     # Theme palettes (Auto/Dark/Light/Midnight/Slate/Warm)
│   ├── controls.rs                  # Win32 child-control wrappers (edit, combo, button)
│   └── gdip.rs                      # GDI+ safe RAII wrappers
├── native/
│   ├── taskbar.rs                   # Taskbar HWND, scale, topmost, fullscreen detection
│   └── displays.rs                  # Monitor bounds / work areas
└── assets/
    ├── app.ico                      # App icon (embedded resource id 1)
    └── icons/
        ├── sunny.ico, cloudy.ico, rain.ico, snow.ico, storm.ico, fog.ico
```

### Data Flow

```
Worker thread
    ├── WeatherClient::get_weather()  → Open-Meteo current weather (temp + WMO code)
    ├── WeatherClient::get_hourly()   → Open-Meteo hourly (precip chance)
    └── PostMessage(WM_WEATHER_RESULT) → App::on_weather_result()
        ├── Widget::set_weather()     → pill repaint
        ├── App::sync_tray_icon()     → tray icon swap (weather_icons::load)
        └── App::sync_tray_tooltip()  → tray tooltip: "Type Temp°C, P% precip"

Pill left-click  → WM_WIDGET_CLICK → App::toggle_forecast() → Forecast popup
Pill right-click → WM_WIDGET_MENU  → App::show_menu()       → context menu
Tray right-click → WM_TRAY (WM_RBUTTONUP) → App::show_menu()

Visibility tick (30 s) + foreground hooks
    taskbar::is_fullscreen_app_on(monitor)
        ├── fullscreen → Widget::hide() + PostMessage(WM_WIDGET_FULLSCREEN) → App closes forecast
        └── normal     → Widget::show() + sync_position()
```

---

## File Layout

### Root

| File | Purpose |
|------|---------|
| `Cargo.toml` | Manifest; Windows crate 0.62 features, serde, embed-resource build-dep; edition 2021 |
| `Cargo.lock` | Locked dependency tree |
| `build.rs` | Embeds `assets/app.ico` as resource id 1 |
| `.cargo/config.toml` | Forces static CRT/UCRT linking (`target-feature=+crt-static`) |
| `make-portable.ps1` | Stops running copies, deletes the old exe, runs `cargo build --release --locked`, verifies system-only imports, copies to `dist/`, optional `-Zip` / `-Run` |
| `NOTES.md` | This document — documentation, changelog, and troubleshooting |
| `EFFICIENCY.md` | Plain-language explanation of storage, update cadence, and footprint |
| `target/release/rusty_weather_pill.exe` | Release build artifact |
| `dist/RustyWeatherPill.exe` | Portable standalone exe |
| `dist/RustyWeatherPill-portable.zip` | Zipped portable package |

### `src/`

| File | Purpose |
|------|---------|
| `main.rs` | Entry; `CreateMutexW` single-instance guard; calls `App::run()` |
| `app.rs` | `App` struct; message loop; weather fetch; tray icon; forecast toggle; settings host |
| `lib.rs` | Module declarations (`app`, `config`, `datetime`, `native`, `net`, `ui`, `util`, `weather`) |
| `config.rs` | `Settings` struct; JSON load/save; sanitize; font/graph/offset clamping |
| `weather.rs` | `WeatherClient`; `WeatherInfo`; `code_to_emoji()`; `weather_description()`; WMO code mapping |
| `net.rs` | `get_string()` over WinHttp |
| `datetime.rs` | `Date` type; Rata Die conversions; ISO-8601 parsing |
| `util.rs` | `write_atomic()` for cache/settings writes |

### `src/ui/`

| File | Purpose |
|------|---------|
| `window.rs` | `WindowHandler` trait; `window::create()` for HWND creation |
| `widget.rs` | `Widget`: pill rendering via GDI+, text+emoji run splitting, drag-to-move, hover, fullscreen hide |
| `icon.rs` | `TrayIcon`: `Shell_NotifyIconW` ADD/MODIFY/DELETE; tooltip; no `uVersion` (classic callback semantics) |
| `weather_icons.rs` | `WeatherIcon` enum; `for_code()` WMO→icon bucket; `load()` builds HICON from embedded `.ico` bytes via `CreateIconFromResourceEx` |
| `forecast.rs` | `Forecast`: layered popup; GDI+ temp curve + area fill; precip bars/%; hour labels; min/max annotations; grip resize; size persisted on close |
| `settings.rs` | `SettingsDialog`: nested modal loop; Win32 edit/combo/button controls; Save/Cancel semantics |
| `theme.rs` | `Palette`: Auto/Dark/Light/Midnight/Slate/Warm; `emoji_color()`; system theme detection |
| `controls.rs` | `label()`, `edit()`, `combo()`, `button()`, `radio_button()`; owner-draw painters (`draw_button`, `draw_combo`, `draw_checkbox`, `draw_radio`); `font_families()` |
| `gdip.rs` | GDI+ RAII: `Bitmap`, `Graphics`, `Font`, `SolidBrush`, `Pen`, `StringFormat`; `draw_lines`, `fill_polygon`, `set_pen_join_round` |

### `src/native/`

| File | Purpose |
|------|---------|
| `taskbar.rs` | `taskbar()` finds `Shell_TrayWnd`; `raise_topmost()` via `SetWindowPos(HWND_TOPMOST)`; `scale_of()` DPI; `tray_notify_left()`; `is_fullscreen_app_on()` |
| `displays.rs` | `Monitor{bounds,work}`; `all()`, `primary()`, `from_window()`, `at_point()` |

### `assets/`

| File | Purpose |
|------|---------|
| `app.ico` | App icon, embedded as resource id 1 |
| `icons/sunny.ico` | Clear weather tray icon |
| `icons/cloudy.ico` | Cloudy/overcast tray icon |
| `icons/rain.ico` | Rain/drizzle tray icon |
| `icons/snow.ico` | Snow tray icon |
| `icons/storm.ico` | Thunderstorm tray icon |
| `icons/fog.ico` | Fog tray icon |

---

## Build & Portable Distribution

### Standard Build

```powershell
cargo build           # debug
cargo build --release # release
cargo test            # unit tests
```

### Portable Build (recommended)

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\make-portable.ps1 -Zip -Run
```

What `make-portable.ps1` does:

1. Stops any running `rusty_weather_pill` / `RustyWeatherPill` processes (prevents an exe lock)
2. Deletes the previous `target/release/rusty_weather_pill.exe` (avoids a stale binary)
3. Runs `cargo build --release --locked` with `.cargo/config.toml` (`target-feature=+crt-static`)
4. Parses the PE import table and **fails** if any VC++ runtime DLL is present (`vcruntime`, `msvcp`, `ucrtbase`, `api-ms-win-crt-*`)
5. Copies the exe to `dist\RustyWeatherPill.exe`
6. Optionally zips to `dist\RustyWeatherPill-portable.zip`
7. Optionally runs a 5-second smoke test

The `.cargo/config.toml` forces static CRT/UCRT linking so the exe runs on a bare Windows 11 with no Visual C++ Redistributable installed.

**Imports verified system-only:** `advapi32`, `api-ms-win-core-synch-l1-2-0`, `dwmapi`, `gdi32`, `gdiplus`, `kernel32`, `ntdll`, `ole32`, `oleaut32`, `shell32`, `user32`, `uxtheme`, `winhttp`.

---

## Running

```powershell
# From the project folder:
.\dist\RustyWeatherPill.exe
```

The app starts minimized to the tray (no `app.ico` shown), fetches weather, shows the pill in the taskbar, and updates the tray icon/tooltip. Right-click the tray icon or pill for the context menu.

---

## Configuration

Settings live at `%LOCALAPPDATA%\WeatherPill\settings.json`. Saves are atomic: a temporary file is written and swapped in, so an interrupted write cannot corrupt the file.

### `Settings` fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `city` | String | `"London"` | Weather city name |
| `widget_anchor` | String | `"Right"` | `"Left"` or `"Right"` — pill anchor side on the taskbar |
| `widget_offset` | i32 | `250` | Distance from anchor edge (logical px), clamped 0..2000 |
| `offset_y` | i32 | `5` | Distance from taskbar top edge (logical px), clamped -1000..1000 |
| `font_family` | String | `"Segoe UI"` | Pill text font family |
| `font_size` | f64 | `12.0` | Pill font size in pt, clamped 6..36 |
| `font_scale_pct` | i32 | `100` | Extra UI scale 80..150 |
| `theme` | String | `"Auto"` | `"Auto" \| "Dark" \| "Light" \| "Midnight" \| "Slate" \| "Warm"` |
| `ui_size` | String | `"Medium"` | Settings dialog size: `"Small" \| "Medium" \| "Large"` |
| `monitor_device_name` | Option<String> | `None` | Target display device (falls back to primary) |
| `graph_w` | i32 | `0` | Forecast graph width (falls back to 480) |
| `graph_h` | i32 | `0` | Forecast graph height (falls back to 240) |

### Cache

Weather cache lives at `%LOCALAPPDATA%\WeatherPill\weather_cache.json`.

- TTL: 45 minutes, keyed by city
- Stores temperature and precipitation chance alongside the display text
- Entries written before v0.1.2 are read with serde defaults and recover the temperature from the cached text

---

## UI Components

### Pill Widget (`ui/widget.rs`)

- Topmost `WS_POPUP` (`WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`) owned by `Shell_TrayWnd`, so it rides the taskbar's z-band
- Rounded rect region (`CreateRoundRectRgn`, radius 8px)
- GDI+ double-buffered render to an in-memory bitmap
- Text runs split into temperature (main font) and emoji (Segoe UI Emoji font) via `split_runs()`
- Emoji font ratio: `EMOJI_FONT_RATIO = 1.38` (1.5× its previous size)
- Left-click → forecast toggle; right-click → context menu
- Drag horizontally to reposition; `widget_offset` persisted on drag commit
- Hides on fullscreen app or video (via foreground/reorder hooks + `SHQueryUserNotificationState`), and posts `WM_WIDGET_FULLSCREEN` so the app closes the forecast popup when fullscreen starts

### Tray Icon (`ui/icon.rs` + `ui/weather_icons.rs`)

- `Shell_NotifyIconW(NIM_ADD)` with `NIF_MESSAGE | NIF_ICON | NIF_TIP`
- Classic callback semantics (no `uVersion`); `lParam` carries the raw mouse message
- Icon is **never** `app.ico` — the tray starts empty and is set on the first weather result via `weather_icons::load(code)`
- `CreateIconFromResourceEx` builds a HICON from embedded `.ico` bytes (parsed ICONDIR + ICONDIRENTRY)
- Icon destroyed on swap and on `Drop` (no handle leaks)
- Tooltip: `"{} {}°C, {}% precip"` (description, temperature, precip chance)

### Forecast Popup (`ui/forecast.rs`)

- Layered `WS_POPUP` (0.95 opacity), positioned above the pill
- GDI+ temperature curve with area fill
- Precipitation probability bars with % labels
- Hour labels, min/max temperature annotations
- Grid lines, Y-axis labels, `°C` suffix
- HTBOTTOMRIGHT grip resize; size persisted on close via `WM_GRAPH_SIZE`
- `Esc` or click-inside closes; also closed when a fullscreen app starts

### Settings Dialog (`ui/settings.rs`)

- Nested modal message loop (`GetMessageW` + `IsDialogMessageW`)
- Sections: LOCATION, WIDGET POSITION, APPEARANCE
- Controls: city edit, offset X/Y edits, font family combo, font size edit, graph scale combo, UI size combo
- Save persists via `Settings::save()`; Cancel reverts (snapshot restore)
- Adaptive control heights by `ui_size` (Small/Medium/Large)
- Combo dropdown item heights set via `CB_SETITEMHEIGHT` (clipping fix for tall fonts)
- Combo dropped width set via `CB_SETDROPPEDWIDTH` (no clipping of long font names)

---

## Weather Service

Source: **Open-Meteo** (free, no API key required)

| Endpoint | URL |
|----------|-----|
| Geocoding | `https://geocoding-api.open-meteo.com/v1/search?name={city}&count=1` |
| Current weather | `https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,weather_code&timezone=auto` |
| Hourly forecast | `...&hourly=temperature_2m,precipitation_probability&forecast_days=2&timezone=auto` |

The WMO `weather_code` (0–99) is the single source of truth: tray icon bucket, pill emoji, and tooltip description all derive from the same `WeatherInfo` struct posted via `WM_WEATHER_RESULT`.

### WMO Code → Icon / Emoji Mapping

| WMO Code | Condition | Tray Icon | Pill Emoji |
|----------|-----------|-----------|------------|
| 0 | Clear | sunny.ico | ☀️ |
| 1–3 | Mainly clear / partly cloudy | cloudy.ico | ⛅ |
| 45, 48 | Fog | fog.ico | ≋ |
| 51–67 | Drizzle / rain / freezing | rain.ico | ☔ |
| 71–77 | Snow | snow.ico | ❄️ |
| 80–82 | Rain showers | rain.ico | ☔ |
| 85, 86 | Snow showers | snow.ico | ❄️ |
| 95–99 | Thunderstorm | storm.ico | ⛈️ |
| other | Unknown | cloudy.ico | 🌡️ |

### `code_to_emoji(code)` — Single Outline Set

```rust
match code {
    0 => "☀️",
    1..=3 => "⛅",
    4..=48 => "≋",
    51..=67 => "☔",
    71..=77 => "❄️",
    80..=82 => "☔",
    85..=86 => "❄️",
    95..=99 => "⛈️",
    _ => "🌡️",
}
```

---

## Testing

`cargo test` runs **25 unit tests**:

- `datetime` (5): Rata Die round-trip, weekdays, leap/month lengths, billing-cycle clamping
- `weather` (6): ISO-8601 parsing round-trip, `code_to_emoji` buckets, coordinate culture-invariance, URI escaping, cached-text temperature recovery
- `config` (2): sanitize defaults, legacy C# file load
- `app` (2): retry backoff and stalled-fetch threshold
- `ui::theme` (3): palette urgency stops and color mixing
- `ui::widget` (3): run splitting (temperature + emoji), glyph classification, plane-1 emoji recognition
- `ui::weather_icons` (2): WMO buckets and all six embedded `.ico` placeholders load to valid HICONs
- `native::taskbar` (2): rectangle coverage rules, fullscreen notification states

---

## Porting Notes (C# → Rust)

| C# Concept | Rust Equivalent |
|------------|-----------------|
| `NotifyIcon` | `Shell_NotifyIconW` + `NOTIFYICONDATAW` |
| `Overlay` (topmost taskbar-owned window) | `Widget` with `Shell_TrayWnd` owner + `WS_EX_TOPMOST` |
| `ForecastForm` | `Forecast` layered popup |
| `SettingsForm` | `SettingsDialog` nested modal loop |
| `WeatherService` | `WeatherClient` (WinHttp + serde_json) |
| `System.Drawing.Graphics` | `ui/gdip.rs` GDI+ wrappers |
| `AppSettings` JSON | `config.rs` `Settings` (serde, snake_case) |
| `NumericUpDown` graph scale | Combo box with `GRAPH_SCALES` |
| Emoji font sets | Single outline set (`code_to_emoji`) |

### Key Win32 Behaviors Leveraged

- **Owned windows inherit z-band:** the widget is owned by `Shell_TrayWnd`, so it stays above the taskbar
- **`TPMPARAMS.rcExclude`:** the menu excludes the taskbar strip so it never renders under it
- **`TPM_RIGHTBUTTON | TPM_VERTICAL`:** context menu alignment
- **`SetForegroundWindow` before `TrackPopupMenuEx`:** required for dismiss-on-elsewhere-click
- **`CBS_OWNERDRAWFIXED` + `CB_SETITEMHEIGHT`:** required to size dropdown items for tall fonts
- **`SHQueryUserNotificationState`:** detects exclusive/D3D fullscreen that window rectangles miss
- **Static CRT (`+crt-static`):** removes the `VCRUNTIME140.dll` / `api-ms-win-crt-*` dependency

---

## Changelog

### [0.1.3] — 2026-11-11

#### Fixed

- **Network-loss recovery:** a stalled HTTP attempt could latch the in-flight guard and block every later refresh (including Force Update) indefinitely. A 45-second watchdog now abandons stalled attempts (late results are discarded by sequence number), a panic in the worker becomes a normal failed result, and a failure retries automatically after 1 minute, backing off to 2, 4, then 5 minutes. Recovery after the connection returns happens within about a minute instead of waiting for the 5-minute tick.
- **Manual cache reset:** the pill/tray context menu now has **Clear Cache**, which deletes `weather_cache.json`, clears the in-memory geocoder memo, and forces a fresh fetch.

#### Changed

- The weather worker catches panics and posts a failed result instead of leaving the refresh latch engaged.
- Refreshed the portable release (`dist/RustyWeatherPill.exe` and `dist/RustyWeatherPill-portable.zip`).

### [0.1.2] — 2026-11-05

#### Fixed

- **Fullscreen overlap:** the pill no longer stays above fullscreen games or videos. It hides while a fullscreen app is active on its monitor and reappears afterwards. Detection now uses the root window, ignores shell windows, requires a same-monitor match, and falls back to `SHQueryUserNotificationState` for exclusive/D3D fullscreen.
- **Forecast popup during fullscreen:** an open forecast popup is now closed when a fullscreen app starts, instead of floating above it.
- **Stale graph hour labels:** `utc_abs()` ignored minutes, so forecast hour labels could be off by up to 59 minutes.
- **Tray tooltip `0°C`:** cached weather now stores `temp_c`, so the tooltip shows the real temperature instead of falling back to 0.

#### Changed

- Restored `edition = "2021"` (the accidental `2026` value prevented builds on the installed toolchain).
- Refreshed the portable release (`dist/RustyWeatherPill.exe` and `dist/RustyWeatherPill-portable.zip`).

### [0.1.1] — Initial Rust port

#### Added

- Complete Rust port of C# `WeatherPill` on a taskbar shell foundation
- Taskbar pill widget with temperature + weather emoji (GDI+ double-buffered rendering)
- System tray icon with context menu (Force Update / Settings / Exit)
- Forecast popup (temperature curve + area fill, precipitation bars/%, hour labels, min/max annotations, grip resize)
- Settings dialog (city, X/Y offsets, font family/size, graph scale, UI size, theme)
- Weather service: Open-Meteo geocoding + forecast + 45-minute file cache
- Single-instance mutex guard
- Portable release build with statically linked MSVC CRT (`make-portable.ps1`)
- Tray icon tooltip: `"{} {}°C, {}% precip"` (description, temperature, precip chance)

#### Changed

- **Web source:** weather data fetched from the Open-Meteo API (`current=temperature_2m,weather_code` + `hourly=temperature_2m,precipitation_probability&forecast_days=2&timezone=auto`)
- **Tray icon:** removed `app.ico` from the tray entirely; the tray starts empty and shows only a weather condition icon (sunny/cloudy/rain/snow/storm/fog) set on the first weather result
- **Emoji system:** collapsed three emoji sets (standard/minimalist/outline) into a single outline set; removed `emoji_set` from `Settings` and the settings dropdown
- **Graph scales:** added the missing `1.75` step to match the C# `NumericUpDown` (0.25 increments): `[1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0]`
- **Emoji mapping:** added explicit buckets for rain showers (80–82 → rain) and snow showers (85–86 → snow) so they no longer fall through to the storm/unknown catchall
- **Emoji size:** `EMOJI_FONT_RATIO` changed from `0.92` to `1.38`, so the default emoji is 1.5× its previous size and aligns with the temperature text borders
- **Settings UI:** added the `ui_size` setting (Small/Medium/Large) that scales dialog fonts and control heights independently of pill text size
- **Settings UI:** combo dropdown item heights now adapt to UI size (32/42/52px) via `CB_SETITEMHEIGHT`; dropped widths widened (fonts 600px, graph 220px, UI size 180px)
- **Settings UI:** layout rewritten with grouped sections (LOCATION, WIDGET POSITION, APPEARANCE), consistent alignment at x=182, compact margins

#### Fixed

- **Tray hover opening the context menu:** `WM_TRAY` fired on every mouse message; now only `lparam == WM_RBUTTONUP` opens the menu
- **Context menu rendering under the taskbar:** removed `attach_to_taskbar()` (`GWLP_HWNDPARENT = Shell_TrayWnd` dragged menus into the taskbar z-band). Replaced `TPM_BOTTOMALIGN` with `TPMPARAMS.rcExclude` set to the taskbar strip, so `TrackPopupMenuEx` never places rows under the taskbar
- **Emoji / tooltip mismatch:** rain showers (80–82) previously rendered the storm emoji while the tooltip said "Rain showers"; both now read from the same WMO bucket
- **Pill emoji alignment:** emoji now vertically centered and 1.5× larger relative to the temperature text

#### Removed

- `app.ico` from the tray icon path (the tray is weather-icon only)
- The three-way emoji choice from the settings dialog (`standard`/`minimalist`/`outline`)
- `attach_to_taskbar()` / `GWLP_HWNDPARENT` owner manipulation on the app window
- Unused constants `CLIENT_X_SAVE`, `BTN_H`, `ODT_BUTTON`

---

## Troubleshooting & Difficult Problems

### 1. Pill staying above fullscreen apps (0.1.2)

**Symptom:** the topmost pill stayed visible over fullscreen games/videos, and an open forecast popup floated above them too.

**Root cause:** the pill is `WS_EX_TOPMOST` and re-raises itself, and the old check only compared `GetForegroundWindow()`'s `GetWindowRect` to the monitor bounds. That misses exclusive/D3D fullscreen (the window rectangle does not describe the render surface) and did not check which monitor the foreground window was on.

**Fix:** `is_fullscreen_app_on()` now resolves the root window (`GetAncestor(GA_ROOT)`), ignores invalid, minimized, invisible, and shell windows (`Shell_TrayWnd`, `Shell_SecondaryTrayWnd`, `WorkerW`, `Progman`), requires `MonitorFromWindow(root)` to match `MonitorFromRect(bounds)` (a game on another monitor must not hide this pill), accepts rectangle coverage with a 2px tolerance, and falls back to `SHQueryUserNotificationState()` for `QUNS_BUSY`, `QUNS_RUNNING_D3D_FULL_SCREEN`, and `QUNS_PRESENTATION_MODE`. On entering fullscreen the widget hides and posts `WM_WIDGET_FULLSCREEN` (`WM_APP + 13`) to the app, which closes the forecast popup; it reappears on exit.

### 2. Context menu rendering under / behind the taskbar

**Symptom:** right-clicking the pill or tray icon produced a context menu partially or fully hidden under the taskbar strip.

**Root cause (two-part):**

1. **`TPM_BOTTOMALIGN` anchored at the cursor inside the taskbar.** The pill lives inside the taskbar strip (taskbar top ≈ y=1385, pill at y≈1393). `TPM_BOTTOMALIGN` set the menu's bottom edge at the cursor, so the menu expanded upward from y≈1410 — but its lower ~25px still fell under the taskbar (which occupies y=1385–1440).
2. **`attach_to_taskbar()` set `GWLP_HWNDPARENT = Shell_TrayWnd` on the app window.** Owned windows inherit the owner's z-band, and so do their popups/context menus. The taskbar repaints its band on activation, painting over anything sharing that band — the menu flashed and vanished or never appeared.

**Fix:** removed `attach_to_taskbar()` entirely (the app window is now unowned). Replaced `TPM_BOTTOMALIGN` with `TPMPARAMS.rcExclude` set to the taskbar strip (computed from monitor `bounds` vs `work`). `TrackPopupMenuEx` now refuses to place any row in the taskbar band, so the menu always renders entirely above it.

**Verification:** real `SendInput` right-clicks on both pill and tray icon produced menus with the bottom edge exactly at `y=1385` (taskbar top) — **0px overlap**.

### 3. Tray hover opening the context menu

**Symptom:** moving the mouse over the tray icon opened the context menu instead of doing nothing.

**Root cause:** `WM_TRAY` (the tray icon callback message) carries the raw mouse message in `lParam` (classic callback semantics, no `uVersion=4`). The handler matched on `WM_TRAY` unconditionally, so `WM_MOUSEMOVE` over the icon triggered the menu.

**Fix:** filter `WM_TRAY` so the menu opens only when `lparam == WM_RBUTTONUP`. Hover and left-click no longer open the menu.

**Verification:** a 12-step hover + wiggle approach at the physical tray icon produced 0 menus; left-click produced 0 menus; right-click produced exactly 1 menu owned by the fresh exe.

### 4. Stale release binary after `cargo build --release`

**Symptom:** deleting `target/release/rusty_weather_pill.exe` then running `cargo build --release` reported "Finished in 0.02s" and restored the old binary (same mtime/hash).

**Root cause:** cargo's fingerprint cache considers the build up-to-date when source fingerprints haven't changed; deleting the output file alone doesn't force recompilation. Cargo restores the previously fingerprinted binary.

**Fix:** touch a source file (e.g. `src/app.rs`) before building to invalidate the fingerprint, then verify via `(Get-FileHash ...).Hash` + `LastWriteTime` that a genuine recompile occurred. `make-portable.ps1` deletes the exe and relies on the `.cargo/config.toml` rustflags (`+crt-static`) to force recompilation.

### 5. Emoji / tray icon mismatch for rain and snow showers

**Symptom:** the pill showed a storm emoji (`⛈️`) while the tray tooltip said "Rain showers".

**Root cause:** `code_to_emoji()` had no explicit bucket for WMO rain showers (80–82) or snow showers (85–86), so they fell through to the storm/default catchall. `weather_description()` *did* map them correctly — hence the text/icon disagreement.

**Fix:** added explicit arms: 80–82 → rain emoji (`☔`), 85–86 → snow emoji (`❄️`). Emoji, tray icon, and tooltip now all read from the same WMO bucket.

### 6. Tray icon showing `app.ico` before the first weather result

**Symptom:** the tray briefly showed the generic `app.ico` on startup before weather data arrived.

**Root cause:** `TrayIcon::new()` loaded the app's embedded icon via `LoadImageW(MAKEINTRESOURCE(1))` and added it immediately.

**Fix:** removed the `app.ico` load from `TrayIcon::new()`. The tray starts with no icon (`NIF_MESSAGE | NIF_TIP` only) and gets its first weather icon via `sync_tray_icon()` on the first weather result.

### 7. Combo dropdown clipping at tall font sizes

**Symptom:** in the settings dialog at Large UI size (22pt font), combo dropdown items clipped the text vertically.

**Root cause (two-part):**

1. `CBS_OWNERDRAWFIXED` combo boxes use a default item height (~16px) unless `CB_SETITEMHEIGHT` is called. The code never called it.
2. `draw_combo()` computed text padding as `item_height / 3` — with a 16px item height that's ~5px, insufficient for a 22pt glyph.

**Fix:** added `combo_set_item_height()` (wraps `CB_SETITEMHEIGHT`) called from `combo_at()` with adaptive heights (32/42/52px for Small/Medium/Large). Changed `draw_combo()` padding from `item_height / 3` to a fixed `6px`.

### 8. Linker lock — `LNK1104: cannot open file ... rusty_weather_pill.exe`

**Symptom:** `cargo build --release` failed because `link.exe` could not write the output exe.

**Root cause:** a previous instance of `rusty_weather_pill.exe` was still running and held a file lock on the exe.

**Fix:** `make-portable.ps1` stops all running copies of `rusty_weather_pill` / `RustyWeatherPill` before deleting and rebuilding.

### 9. Stale forecast hours and `0°C` tray tooltip (0.1.2)

**Symptom:** forecast hour labels were occasionally one hour behind, and the tray tooltip showed `0°C` after a cache hit.

**Root cause:** `utc_abs()` converted a date to seconds but ignored minutes and seconds, rounding the forecast to the hour. `CacheEntry` stored the display text but not `temp_c`, so the tooltip had no numeric temperature and fell back to `0`.

**Fix:** `utc_abs()` now includes minutes and seconds; `CacheEntry` gained `#[serde(default)] temp_c` and `precip_pct`, and the tooltip uses the cached temperature with `temp_from_text()` as a fallback for older cache files.

### 10. PowerShell `Add-Type` / `SendInput` testing pitfalls

**Symptom:** test scripts using `Add-Type` to call Win32 APIs failed with "Unable to find type [RCx]" without showing the underlying C# compile error; `SendInput` structs also failed to compile.

**Root causes:**

- `FindWindowW` string marshaling needs `CharSet=Unicode` or it silently fails to find windows.
- C# snippets needed `using System.Collections.Generic;` and other namespaces.
- Variable redeclaration (e.g. `RECT cursor;` declared twice).
- `INPUT` contains a union (`MOUSEINPUT` / `KEYBDINPUT`) at field offset 0.

**Fix:** reliable window lookup uses `EnumWindows` + `GetClassNameW` + a pid filter; include the full `using` block; avoid variable shadowing; declare the union with `[StructLayout(LayoutKind.Explicit)]` and `[FieldOffset(0)]` on both members, and pass `Marshal.SizeOf(typeof(INPUT))` to `SendInput`. Prefer `SendInput` over `mouse_event`.

### 11. Accidental `edition = "2026"` broke builds (0.1.2)

**Symptom:** `cargo test`/`cargo build` failed with "this version of Cargo is older than the 2026 edition, and only supports 2015, 2018, 2021, and 2024".

**Root cause:** `Cargo.toml` was manually changed to `edition = "2026"` after the last successful build, but the installed toolchain (cargo/rustc 1.99.1) only supports editions through 2024.

**Fix:** restored `edition = "2021"`. The crate uses no edition-specific features, and 2021 builds on any current toolchain.

### 12. Refresh stuck after a network loss (0.1.3)

**Symptom:** after the connection dropped and returned, the pill kept showing `Err`, and Force Update did nothing until the app was restarted.

**Root cause:** `refreshing` was set before the worker started and only cleared in `on_weather_result`. If a WinHTTP attempt never returned (network change, dead proxy, or a worker panic), the flag stayed latched, so every later attempt — including Force Update — was skipped. The failure path also relied on the 5-minute weather timer.

**Fix:** the in-flight state now carries a start time and a sequence number. An attempt older than 45 seconds is abandoned and replaced; its late result is discarded. The worker wraps the fetch in `catch_unwind` and always posts a result. A failed result schedules `TIMER_RETRY` (1 minute, doubling to 5), and a success kills that timer and resets the delay. The **Clear Cache** menu item deletes `weather_cache.json`, clears the geocoder memo, and forces a fetch for the manual fallback.

---

*RustyWeatherPill v0.1.3 — documentation, changelog, and troubleshooting consolidated into one file.*
