# How RustyWeatherPill Works — Simple Explanation

## Where Information Is Stored

RustyWeatherPill keeps everything in one place: `%LOCALAPPDATA%\WeatherPill\`.

There are only two files:

| File | What it holds |
|------|---------------|
| `settings.json` | Your city, font, graph size, offsets, theme, and UI size preferences |
| `weather_cache.json` | The last weather result (temperature, condition, coordinates, time it was fetched) |

There is **no database**, **no registry changes**, and **no hidden folders** scattered around your system. If you want to back up or move your settings, just copy that one folder.

When the app saves a file, it writes to a temporary file first, then swaps it in. This means if the power cuts or the app crashes mid-save, your settings won't be corrupted.

---

## How Often It Updates

| What | How often |
|------|-----------|
| Weather refresh | Every 5 minutes (while the app is running) |
| Weather cache lifetime | 45 minutes |
| Position / visibility check | Every 30 seconds (hides the pill while a fullscreen game or video is active) |
| Forecast popup | Only when you left-click the pill; closes when you click away or press Esc |

**Important:** If the weather server is unreachable, the app simply keeps showing the last good reading it successfully fetched. It does not keep retrying in a tight loop that would drain your battery or bandwidth.

---

## Why It Is So Light

### No Web Browser Engine
Many modern desktop apps are really just websites wrapped in a heavy browser engine (Electron). They use hundreds of megabytes of RAM just to show a clock. RustyWeatherPill uses **native Windows drawing (GDI+)** — the same technology Windows itself uses for window borders and text. It starts fast and stays tiny.

### No Background Threads Running Constantly
The app has one main message loop. Weather fetching happens on a short-lived worker thread that shuts down as soon as the data comes back. There is no chatty background process watching your mouse or polling the internet.

### No Extra Software Required
It bundles the Visual C++ runtime inside the `.exe`, so it does not depend on you having the "Visual C++ Redistributable" installed. It is one file, no installer, nothing to add to Windows.

### Smart Redrawing
The pill only repaints when something actually changes — new weather, you moved it, or you hovered over it. It does not redraw itself 60 times a second like a video game.

### Tiny Memory Footprint
Because it is a single small executable with no embedded browser, no image libraries beyond what Windows already provides, and no background polling, it uses a fraction of the RAM a typical "widget" app would.

---

## Bottom Line

RustyWeatherPill stores your settings in two simple JSON files, checks the weather every five minutes (and remembers it for 45), and stays light by using Windows' own drawing tools instead of shipping a whole web browser inside the app.
