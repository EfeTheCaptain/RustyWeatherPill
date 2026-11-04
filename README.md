# RustyWeatherPill 🌤️

A native Windows taskbar weather pill written in Rust. Zero `.NET` dependencies, statically-linked CRT, and completely portable. 

Built from scratch using the `windows` crate (Win32 API FFI) and GDI+ for high-performance, low-memory rendering.

## ✨ Features & Screenshots

- **Taskbar Pill:** Shows current temperature and weather condition emoji right on your taskbar.
- **Dynamic Tray Icon:** The system tray icon automatically updates to match the current weather.
- **Forecast Popup:** Left-click the pill for a GDI+ temperature graph and precipitation probability bars.
- **Portable:** Single `.exe` (no Visual C++ Redistributable required).
- **Theming:** 6 themes (Auto, Dark, Light, Midnight, Slate, Warm) with auto-detection for Windows dark/light mode.

**Dynamic Tray Icon** (Changes according to the Weather type):
<img width="141" height="69" alt="2026-11-04_15-09" src="https://github.com/user-attachments/assets/bb9f11ef-3ddd-4710-bf49-d0dca4f5b890" />

**UI and the Graph** (Graph can be toggled by clicking on the widget pill, doesn't go away unless you click on the widget or the graph itself):
<img width="1086" height="536" alt="2026-11-04_15-08" src="https://github.com/user-attachments/assets/9a90bb5e-590c-481b-b17a-49f85d27582c" />


## 🚀 Download & Run

1. Go to the [Releases](../../releases) page.
2. Download the latest `RustyWeatherPill.exe` (or the `.zip`).
3. Run it. It will automatically dock to your taskbar and appear in the system tray.


---

## ⚡ Under the Hood (Why it's so light)

Most modern desktop apps are just web browsers in disguise (Electron). RustyWeatherPill is different. It is built to be light, fast, and invisible until you need it.

### No Web Browser Engine
It uses **native Windows drawing (GDI+)** — the same technology Windows itself uses for window borders and text. It starts fast and stays tiny.

### No Extra Software Required
It bundles the Visual C++ runtime inside the `.exe` (Static CRT), so it does not depend on you having the "Visual C++ Redistributable" installed. It is one file, no installer, no registry clutter.

### It Does Not Poll the Weather Constantly
- **Weather refresh:** Once every 5 minutes (while the app is running).
- **Cache lifetime:** 45 minutes (re-opening the app uses the cache, saving bandwidth).
- **Position check:** Every 30 seconds (just to make sure the pill hasn't fallen behind a fullscreen game).
- **Forecast popup:** Only opens on left-click and closes from memory immediately.

### Smart Redrawing
The pill only repaints when something actually changes — new weather, you moved it, or you hovered over it. It does not redraw itself 60 times a second like a video game. It uses double-buffering to ensure no flickering.

### Where Information Is Stored
Everything lives in `%LOCALAPPDATA%\WeatherPill\`:
- `settings.json`: Your preferences.
- `weather_cache.json`: The last weather result.

There is no database, no registry changes, and no hidden folders. If the power cuts or the app crashes mid-save, atomic writes ensure your settings won't be corrupted.

---

## 🛠️ Building from Source

Requires Rust and MSVC Build Tools.

```powershell
# Standard build
cargo build --release

# Portable build (static CRT, no dependencies)
powershell -NoProfile -ExecutionPolicy Bypass -File .\make-portable.ps1 -Zip
