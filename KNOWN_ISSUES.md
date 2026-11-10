# Known Issues

Here are the currently known issues and their workarounds for RustyWeatherPill.

### 1. Tray Icon Tooltip May Not Update Immediately
**Symptom:** The tooltip (hover text) on the tray icon might occasionally show stale weather data.
**Workaround:** Right-click the tray icon (or the pill) and select **Force Update** from the context menu. This will manually trigger a refresh and update the tooltip.

### 2. Ambiguous City Names
**Symptom:** If you enter a city name that exists in multiple countries or states (e.g., "Springfield" or "Paris"), the app may resolve to the wrong location.
**Cause:** This is a limitation of the free Open-Meteo Geocoding API, which returns the first result it finds. 
**Workaround:** If this happens, try entering a more specific query (e.g., "Paris, France" or "Springfield, IL") to help the API find the correct city.
