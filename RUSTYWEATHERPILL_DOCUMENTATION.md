# RustyWeatherPill — Documentation

**Package:** `rusty_weather_pill` **v0.1.0**  
**License:** MIT  
**Location:** `F:\AI\APPS\RustyWeatherPill\`  
**Description:** Rust port of the C# `WeatherPill` taskbar weather app on a taskbar shell foundation. A standalone, portable Windows 11 tray widget that shows current weather in the taskbar and a forecast popup.

---

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [File Layout](#file-layout)
4. [Build & Portable Distribution](#build--portable-distribution)
5. [Configuration](#configuration)
6. [UI Components](#ui-components)
7. [Weather Service](#weather-service)
8. [Tray Icon](#tray-icon)
9. [Forecast Popup](#forecast-popup)
10. [Settings Dialog](#settings-dialog)
11. [Dependencies](#dependencies)
12. [Testing](#testing)
13. [Porting Notes (C# → Rust)](#porting-notes-c--to-rust)

---

## Overview

RustyWeatherPill is a native Windows taskbar weather pill written in Rust using the `windows` crate (Win32 API FFI) and GDI+ for rendering. It:

- Lives as a pill widget inside the Windows taskbar strip
- Shows current temperature + weather condition emoji
- Updates the system tray icon to match the weather condition
- Opens a forecast popup (temperature curve + precipitation bars) on pill left-click
- Opens a context menu (Force Update / Settings / Exit) on pill right-click
- Runs as a single instance (mutex-guarded)
- Stores settings and cache in `%LOCALAPPDATA%\WeatherPill\`
- Is distributed as a portable, single-exe release with statically-linked MSVC CRT (no Visual C++ Redistributable required)

---

## Architecture

```
rusty_weather_pill.exe
├── main.rs                          # Entry point; single-instance mutex
├── app.rs                           # App orchestrator (window, tray, widget, weather, forecast, settings)
├── lib.rs                           # Module root
├── config.rs                        # Settings (JSON, %LOCALAPPDATA%\WeatherPill\settings.json)
├── weather.rs                       # WeatherClient: Open-Meteo API, geocoding, forecast, WMO codes, emoji
├── net.rs                           # HTTP client (WinHttp)
├── datetime.rs                      # Date utilities (Rata Die)
├── util.rs                          # Atomic file writes
├── ui/
│   ├── mod.rs
│   ├── window.rs                    # HWND creation wrapper
│   ├── widget.rs                    # Taskbar pill widget (rendering, drag, hover, resize)
│   ├── icon.rs                      # Tray icon (Shell_NotifyIcon, tooltip, icon swap)
│   ├── weather_icons.rs             # Embedded .ico assets keyed to WMO codes
│   ├── forecast.rs                  # Forecast popup (GDI+ graph, precip bars)
│   ├── settings.rs                  # Settings dialog (Win32 controls)
│   ├── theme.rs                     # Theme palettes (Auto/Dark/Light/Midnight/Slate/Warm)
│   ├── controls.rs                  # Win32 child-control wrappers (edit, combo, button)
│   └── gdip.rs                      # GDI+ safe RAII wrappers
├── native/
│   ├── mod.rs
│   ├── taskbar.rs                   # Taskbar HWND, scale, topmost, visibility
│   └── displays.rs                  # Monitor bounds / work areas
└── assets/
    ├── app.ico                      # App icon (embedded resource id 1)
    └── icons/
        ├── sunny.ico, cloudy.ico, rain.ico, snow.ico, storm.ico, fog.ico
```

### Data Flow

```
Weather fetch (worker thread)
    └── WeatherClient::get_weather()  → Open-Meteo current weather (temp + WMO code)
    └── WeatherClient::get_hourly()   → Open-Meteo hourly (precip chance)
    └── PostMessage(WM_WEATHER_RESULT) → App::on_weather_result()
        ├── App.text / App.code updated
        ├── Widget::set_weather()     → pill repaint
        ├── App::sync_tray_icon()     → tray icon swap (weather_icons::load)
        └── App::sync_tray_tooltip()  → tray tooltip: "Type Temp°C, P% precip"

Pill left-click → WM_WIDGET_CLICK → App::toggle_forecast() → Forecast popup
Pill right-click → WM_WIDGET_MENU → App::show_menu() → context menu
Tray right-click → WM_TRAY (WM_RBUTTONUP) → App::show_menu()
```

---

## File Layout

### Root
| File | Purpose |
|------|---------|
| `Cargo.toml` | Manifest; Windows crate 0.62 features, serde, embed-resource build-dep |
| `build.rs` | Embeds `assets/app.ico` as resource id 1 |
| `make-portable.ps1` | PowerShell script: stops running copies, deletes old exe, `cargo build --release --locked`, verifies imports are system-only, copies to `dist/`, optional zip |
| `make-portable.ps1` | Verifies no `vcruntime`, `msvcp`, `ucrtbase`, or `api-ms-win-crt-*` imports |
| `Cargo.lock` | Locked dependency tree |
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
| `icon.rs` | `TrayIcon`: `Shell_NotifyIconW` ADD/MODIFY/DELETE; tooltip; no uVersion (classic callback semantics) |
| `weather_icons.rs` | `WeatherIcon` enum; `for_code()` WMO→icon bucket; `load()` builds HICON from embedded `.ico` bytes via `CreateIconFromResourceEx` |
| `forecast.rs` | `Forecast`: layered popup; GDI+ temp curve + area fill; precip bars/%; hour labels; min/max annotations; grip resize; size persisted on close |
| `settings.rs` | `SettingsDialog`: nested modal loop; Win32 edit/combo/button controls; Save/Cancel semantics |
| `theme.rs` | `Palette`: Auto/Dark/Light/Midnight/Slate/Warm; `emoji_color()`; system theme detection |
| `controls.rs` | `label()`, `edit()`, `combo()`, `button()`, `radio_button()`; owner-draw painters (`draw_button`, `draw_combo`, `draw_checkbox`, `draw_radio`); `font_families()` |
| `gdip.rs` | GDI+ RAII: `Bitmap`, `Graphics`, `Font`, `SolidBrush`, `Pen`, `StringFormat`; `draw_lines`, `fill_polygon`, `set_pen_join_round` |

### `src/native/`
| File | Purpose |
|------|---------|
| `taskbar.rs` | `taskbar()` find `Shell_TrayWnd`; `raise_topmost()` via `SetWindowPos(HWND_TOPMOST)`; `scale_of()` DPI; `tray_notify_left()` |
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
cargo build          # debug
cargo build --release # release
cargo test           # unit tests
```

### Portable Build (recommended)
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\make-portable.ps1 -Zip -Run
```

What `make-portable.ps1` does:
1. Stops any running `rusty_weather_pill` / `RustyWeatherPill` processes (prevents exe lock)
2. Deletes the previous `target/release/rusty_weather_pill.exe` (avoids stale binary)
3. Runs `cargo build --release --locked` with `.cargo/config.toml` (`target-feature=+crt-static`)
4. Parses the PE import table and **fails** if any VC++ runtime DLL is present (`vcruntime`, `msvcp`, `ucrtbase`, `api-ms-win-crt-*`)
5. Copies the exe to `dist\RustyWeatherPill.exe`
6. Optionally zips to `dist\RustyWeatherPill-portable.zip`
7. Optionally runs a 5-second smoke test

The `.cargo/config.toml` forces static CRT/UCRT linking so the exe runs on a bare Windows 11 with no Visual C++ Redistributable installed.

**Imports verified system-only:** `advapi32`, `api-ms-win-core-synch-l1-2-0`, `dwmapi`, `gdi32`, `gdiplus`, `kernel32`, `ntdll`, `ole32`, `oleaut32`, `shell32`, `user32`, `uxtheme`, `winhttp`.

---

## Configuration

Settings live at: `%LOCALAPPDATA%\WeatherPill\settings.json`

### `Settings` struct fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `city` | String | `"London"` | Weather city name |
| `widget_anchor` | String | `"Right"` | `"Left"` or `"Right"` — pill anchor side on taskbar |
| `widget_offset` | i32 | `250` | Distance from anchor edge (logical px), clamped 0..2000 |
| `offset_y` | i32 | `5` | Distance from taskbar top edge (logical px), clamped -1000..1000 |
| `font_family` | String | `"Segoe UI"` | Pill text font family |
| `font_size` | f64 | `12.0` | Pill font size in pt, clamped 6..36 |
| `font_scale_pct` | i32 | `100` | Extra UI scale 80..150 |
| `theme` | String | `"Auto"` | `"Auto" \| "Dark" \| "Light" \| "Midnight" \| "Slate" \| "Warm"` |
| `emoji_set` | String | *(removed)* | Previously `standard\|minimalist\|outline`; now single outline set |
| `ui_size` | String | `"Medium"` | Settings dialog size: `"Small" \| "Medium" \| "Large"` |
| `monitor_device_name` | Option<String> | `None` | Target display device |
| `graph_w` | i32 | `0` | Forecast graph width (falls back to 480) |
| `graph_h` | i32 | `0` | Forecast graph height (falls back to 240) |

### Cache

Weather cache at: `%LOCALAPPDATA%\WeatherPill\weather_cache.json`  
TTL: 45 minutes. Keyed by city.

---

## UI Components

### Pill Widget (`ui/widget.rs`)

- Topmost `WS_POPUP` owned by `Shell_TrayWnd` (rides taskbar z-band)
- Rounded rect region (`CreateRoundRectRgn`, radius 8px)
- GDI+ double-buffered render to in-memory bitmap
- Text runs split into temperature (main font) and emoji (Segoe UI Emoji font) via `split_runs()`
- Emoji font ratio: `EMOJI_FONT_RATIO = 1.38` (1.5× default size vs previous)
- Left-click → forecast toggle; Right-click → context menu
- Drag horizontally to reposition; `widget_offset` persisted on drag commit
- Hides on fullscreen app or video (via `EVENT_SYSTEM_FOREGROUND` + `EVENT_OBJECT_REORDER` hooks + `SHQueryUserNotificationState`), and closes the forecast popup when fullscreen starts

### Tray Icon (`ui/icon.rs` + `ui/weather_icons.rs`)

- `Shell_NotifyIconW(NIM_ADD)` with `NIF_MESSAGE \| NIF_ICON \| NIF_TIP`
- Classic callback semantics (no `uVersion`); `lParam` carries raw mouse message
- Icon is **never** `app.ico` — starts empty, set on first weather result via `weather_icons::load(code)`
- `CreateIconFromResourceEx` builds HICON from embedded `.ico` bytes (parsed ICONDIR + ICONDIRENTRY)
- Icon destroyed on swap and on `Drop` (no handle leaks)
- Tooltip: `"{} {}°C, {}% precip"` (description, temperature, precip chance)

### Forecast Popup (`ui/forecast.rs`)

- Layered `WS_POPUP` (0.95 opacity), positioned above the pill
- GDI+ temperature curve with area fill
- Precipitation probability bars with % labels
- Hour labels, min/max temperature annotations
- Grid lines, Y-axis labels, `°C` suffix
- HTBOTTOMRIGHT grip resize; size persisted on close via `WM_GRAPH_SIZE`
- `Esc` or click-inside closes

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

Source: **Open-Meteo** (free, no API key)

- Geocoding: `https://geocoding-api.open-meteo.com/v1/search?name={city}&count=1`
- Forecast: `https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code&timezone=auto`
- Hourly: `...&hourly=temperature_2m,precipitation_probability&forecast_days=2&timezone=auto`

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

`cargo test` — 20 unit tests across:
- `weather`: ISO-8601 parsing round-trip, `code_to_emoji` buckets, coordinate culture-invariance, URI escaping
- `config`: sanitize defaults, legacy C# file load
- `widget`: run splitting (temperature + emoji), minimalist glyph classification, plane-1 emoji recognition
- `weather_icons`: all six embedded `.ico` placeholders load to valid `HICON`

---

## Porting Notes (C# → Rust)

| C# Concept | Rust Equivalent |
|------------|-----------------|
| `NotifyIcon` | `Shell_NotifyIconW` + `NOTIFYICONDATAW` |
| `Overlay` (topmost taskbar-owned window) | `Widget` with `Shell_TrayWnd` parent + `WS_EX_TOPMOST` |
| `ForecastForm` | `Forecast` layered popup |
| `SettingsForm` | `SettingsDialog` nested modal loop |
| `WeatherService` | `WeatherClient` (WinHttp + serde_json) |
| `System.Drawing.Graphics` | `ui/gdip.rs` GDI+ wrappers |
| `AppSettings` JSON | `config.rs` `Settings` (serde, snake_case) |
| `NumericUpDown` graph scale | Combo box with `GRAPH_SCALES` |
| Emoji font sets | Single outline set (`code_to_emoji`) |

### Key Win32 Behaviors Leveraged
- **Owned windows inherit z-band**: Widget owned by `Shell_TrayWnd` stays above taskbar
- **`TPMPARAMS.rcExclude`**: Menu excludes taskbar strip so it never renders under it
- **`TPM_RIGHTBUTTON \| TPM_VERTICAL`**: Context menu alignment
- **`SetForegroundWindow` before `TrackPopupMenuEx`**: Required for dismiss-on-elsewhere-click
- **`CBS_OWNERDRAWFIXED` + `CB_SETITEMHEIGHT`**: Required to size dropdown items for tall fonts
- **Static CRT (`+crt-static`)**: Removes `VCRUNTIME140.dll` / `api-ms-win-crt-*` dependency

---

## Running

```powershell
# From the project folder:
.\dist\RustyWeatherPill.exe
```

The app starts minimized to the tray (no app.ico shown), fetches weather, shows the pill in the taskbar, and updates the tray icon/tooltip. Right-click the tray icon or pill for the context menu.

---

## Difficult Problems & Solutions

### 1. Context menu rendering under / behind the taskbar

**Symptom:** Right-clicking the pill or tray icon produced a context menu that was partially or fully hidden under the taskbar strip.

**Root cause (two-part):**

1. **`TPM_BOTTOMALIGN` anchored at the cursor inside the taskbar.** The pill lives inside the taskbar strip (taskbar top ≈ y=1385, pill at y≈1393). `TPM_BOTTOMALIGN` set the menu's bottom edge at the cursor, so the menu expanded upward from y≈1410 — but its lower ~25px still fell under the taskbar (which occupies y=1385–1440).

2. **`attach_to_taskbar()` set `GWLP_HWNDPARENT = Shell_TrayWnd` on the app window.** Owned windows inherit the owner's z-band, and so do their popups/context menus. The taskbar repaints its band on activation, painting over anything sharing that band — the menu flashed and vanished or never appeared.

**Fix:** Removed `attach_to_taskbar()` entirely (the app window is now unowned). Replaced `TPM_BOTTOMALIGN` with `TPMPARAMS.rcExclude` set to the taskbar strip (computed from monitor `bounds` vs `work`). `TrackPopupMenuEx` now refuses to place any row in the taskbar band, so the menu always renders entirely above it.

**Verification:** Real `SendInput` right-clicks on both pill and tray icon produced menus with bottom edge exactly at `y=1385` (taskbar top) — **0px overlap**.

---

### 2. Tray hover opening the context menu

**Symptom:** Moving the mouse over the tray icon opened the context menu instead of doing nothing.

**Root cause:** `WM_TRAY` (the tray icon callback message) carries the raw mouse message in `lParam` (classic callback semantics, no `uVersion=4`). The handler matched on `WM_TRAY` unconditionally, so `WM_MOUSEMOVE` over the icon triggered the menu.

**Fix:** Filter `WM_TRAY` so the menu opens only when `lparam == WM_RBUTTONUP`. Hover and left-click no longer open the menu.

**Verification:** 12-step hover + wiggle approach at the physical tray icon produced 0 menus; left-click produced 0 menus; right-click produced exactly 1 menu owned by the fresh exe.

---

### 3. Stale release binary after `cargo build --release`

**Symptom:** Deleting `target/release/rusty_weather_pill.exe` then running `cargo build --release` reported "Finished in 0.02s" and restored the old binary (same mtime/hash).

**Root cause:** Cargo's fingerprint cache considers the build up-to-date when source fingerprints haven't changed; deleting the output file alone doesn't force recompilation. Cargo restores the previously fingerprinted binary.

**Fix:** Touch a source file (e.g. `src/app.rs`) before building to invalidate the fingerprint, then verify via `(Get-FileHash ...).Hash` + `LastWriteTime` that a genuine recompile occurred. `make-portable.ps1` deletes the exe, touches nothing explicitly but relies on the `.cargo/config.toml` rustflags change (`+crt-static`) to force recompilation.

---

### 4. Emoji / tray icon mismatch for rain showers and snow showers

**Symptom:** The pill showed a storm emoji (`⛈️`) while the tray tooltip said "Rain showers".

**Root cause:** `code_to_emoji()` had no explicit bucket for WMO rain showers (80–82) or snow showers (85–86), so they fell through to the storm/default catchall. `weather_description()` *did* map them correctly — hence text/icon disagreement.

**Fix:** Added explicit arms:
- 80–82 → rain emoji (`☔`)
- 85–86 → snow emoji (`❄️`)

Now emoji, tray icon, and tooltip all read from the same WMO code bucket.

---

### 5. Tray icon showing `app.ico` before first weather result

**Symptom:** The tray briefly showed the generic `app.ico` on startup before weather data arrived.

**Root cause:** `TrayIcon::new()` loaded the app's embedded icon via `LoadImageW(MAKEINTRESOURCE(1))` and added it immediately.

**Fix:** Removed the app.ico load from `TrayIcon::new()`. The tray starts with no icon (`NIF_MESSAGE \| NIF_TIP` only) and gets its first weather icon via `sync_tray_icon()` on the first weather result. `app.ico` is no longer involved in the tray at all.

---

### 6. Combo dropdown clipping at tall font sizes

**Symptom:** In the settings dialog at Large UI size (22pt font), combo dropdown items clipped the text vertically.

**Root cause (two-part):**

1. `CBS_OWNERDRAWFIXED` combo boxes use a default item height (~16px) unless `CB_SETITEMHEIGHT` is called. The code never called it.
2. `draw_combo()` computed text padding as `item_height / 3` — with a 16px item height that's ~5px, insufficient for a 22pt glyph.

**Fix:** Added `combo_set_item_height()` (wraps `CB_SETITEMHEIGHT`) called from `combo_at()` with adaptive heights (32/42/52px for Small/Medium/Large). Changed `draw_combo()` padding from `item_height / 3` to a fixed `6px`.

---

### 7. Linker lock — `LNK1104: cannot open file ... rusty_weather_pill.exe`

**Symptom:** `cargo build --release` failed because `link.exe` could not write the output exe.

**Root cause:** A previous instance of `rusty_weather_pill.exe` was still running and held a file lock on the exe.

**Fix:** `make-portable.ps1` stops all running copies of `rusty_weather_pill` / `RustyWeatherPill` before deleting and rebuilding.

---

### 8. PowerShell `Add-Type` compile failures during real-input testing

**Symptom:** Test scripts using `Add-Type` to call Win32 APIs failed with "Unable to find type [RCx]" without showing the underlying C# compile error.

**Root causes:**
- `FindWindowW` string marshaling needs `CharSet=Unicode` or it silently fails to find windows.
- C# snippets needed `using System.Collections.Generic;` and other namespaces.
- Variable redeclaration (e.g. `RECT cursor;` declared twice).
- `SendInput` struct layout needed explicit `[FieldOffset]` handling for the union.

**Fix:** Reliable window lookup = `EnumWindows` + `GetClassNameW` + pid filter; include full `using` block; avoid variable shadowing; use `SendInput` for real mouse input rather than `mouse_event` or `mouse_event` equivalents.

---

### 9. `Add-Type` / C# union struct for `INPUT`

**Symptom:** `SendInput` C# struct failed to compile because `INPUT` contains a union (`MOUSEINPUT` / `KEYBDINPUT`) at field offset 0.

**Fix:** Declared `[StructLayout(LayoutKind.Explicit)]` on the union wrapper with `[FieldOffset(0)]` on both members, and passed `Marshal.SizeOf(typeof(INPUT))` as the struct size to `SendInput`.

---

## Appendix: Critical web source change

The weather data source is the **Open-Meteo** API (no API key required):

- Geocoding: `https://geocoding-api.open-meteo.com/v1/search?name={city}&count=1`
- Current weather: `https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,weather_code&timezone=auto`
- Hourly forecast: `...&hourly=temperature_2m,precipitation_probability&forecast_days=2&timezone=auto`

The WMO `weather_code` (0–99) drives both the tray icon bucket and the pill emoji. This is a single source of truth — icon, tooltip description, and pill emoji all derive from the same code parsed in one `WeatherInfo` struct posted via `WM_WEATHER_RESULT`.