# RustyWeatherPill 🌤️

A native Windows taskbar weather pill written in Rust. Zero `.NET` dependencies, statically-linked CRT, and completely portable. 

Built from scratch using the `windows` crate (Win32 API FFI) and GDI+ for high-performance, low-memory rendering.

## ✨ Features
- **Taskbar Pill:** Shows current temperature and weather condition emoji right on your taskbar.
- **Dynamic Tray Icon:** The system tray icon automatically updates to match the current weather.
- **Forecast Popup:** Left-click the pill for a GDI+ temperature graph and precipitation probability bars.
- **Portable:** Single `.exe` (no Visual C++ Redistributable required).
- **Theming:** Auto, Dark, Light, Midnight, Slate, and Warm themes.

## 🚀 Download & Run
1. Go to the [Releases](../../releases) page.
2. Download the latest `RustyWeatherPill.exe` (or the `.zip`).
3. Run it. It will automatically dock to your taskbar and appear in the system tray.

## 🛠️ Building from Source
Requires Rust and MSVC Build Tools.
```powershell
# Standard build
cargo build --release

# Portable build (static CRT, no dependencies)
powershell -NoProfile -ExecutionPolicy Bypass -File .\make-portable.ps1 -Zip
