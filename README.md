# RustyWeatherPill 🌤️

A native Windows taskbar weather pill written in Rust. Zero `.NET` dependencies, statically-linked CRT, and completely portable.

Built from scratch using the `windows` crate (Win32 API FFI) and GDI+ for high-performance, low-memory rendering.

---

## ✨ Features

| Feature | Description |
|---------|-------------|
| **Taskbar Pill** | Shows current temperature and weather emoji right on your taskbar |
| **Dynamic Tray Icon** | System tray icon automatically updates to match the weather |
| **Forecast Popup** | Left-click the pill for a GDI+ temperature graph and precipitation bars |
| **Portable** | Single `.exe` — no Visual C++ Redistributable required |

---

## 📸 Screenshots

<p align="center">
  <img width="141" height="69" alt="Dynamic Tray Icon" src="https://github.com/user-attachments/assets/bb9f11ef-3ddd-4710-bf49-d0dca4f5b890" />
  <br>
  <em>Dynamic tray icon — changes with the weather</em>
</p>

<p align="center">
  <img width="854" height="477" alt="Forecast Popup" src="https://github.com/user-attachments/assets/088e0da1-3843-4947-8049-52c6a80faea4" />
  <br>
  <em>UI and graph — toggled by clicking the widget pill</em>
</p>

---

## 🚀 Download & Run

1. Go to the **[Releases](../../releases)** page.
2. Download the latest `RustyWeatherPill.exe` (or the `.zip`).
3. Run it — it docks to your taskbar and appears in the system tray.

---

## ⚡ Under the Hood

<p align="center">
  <img width="1000" height="193" alt="Lightweight Design" src="https://github.com/user-attachments/assets/2fd936a2-178c-47e9-8c4e-ac1341150673" />
</p>

Most modern desktop apps are just web browsers in disguise (Electron). RustyWeatherPill is different — built to be light, fast, and invisible until you need it.

### No Web Browser Engine

It uses **native Windows drawing (GDI+)** — the same technology Windows itself uses for window borders and text. It starts fast and stays tiny.

### No Extra Software Required

It bundles the Visual C++ runtime inside the `.exe` (Static CRT), so it does not depend on you having the "Visual C++ Redistributable" installed. One file, no installer, no registry clutter.

### It Does Not Poll the Weather Constantly

- **Weather refresh:** Once every 5 minutes
- **Cache lifetime:** 45 minutes
- **Position check:** Every 30 seconds
- **Forecast popup:** Opens on left-click, closes from memory immediately

### Smart Redrawing

The pill only repaints when something changes — new weather, movement, or hover. It uses double-buffering to ensure no flickering.

### Where Information Is Stored

Everything lives in `%LOCALAPPDATA%\WeatherPill\`:
- `settings.json`: Your preferences
- `weather_cache.json`: The last weather result

No database, no registry changes, no hidden folders. Atomic writes ensure your settings won't be corrupted if the app crashes mid-save.

---

## 🛠️ Building from Source

Requires Rust and MSVC Build Tools.

```powershell
# Standard build
cargo build --release

# Portable build (static CRT, no dependencies)
powershell -NoProfile -ExecutionPolicy Bypass -File .\make-portable.ps1 -Zip
```

---

<p align="center">
  <em>Built with ❤️ using Rust and the Windows API</em>
</p>
