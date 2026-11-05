//! App orchestrator, port of WeatherPill/Program.cs + Overlay orchestration onto the
//! TaskbarPray AppHost pattern: a hidden top-level window owning the widget, timers, and the
//! weather pipeline. The window is NOT message-only — WM_SETTINGCHANGE and the TaskbarCreated
//! broadcast are only delivered to real top-level windows.

use crate::config::Settings;
use crate::ui::forecast::{Forecast, WM_FORECAST_RESULT, WM_GRAPH_SIZE};
use crate::ui::icon::{TrayIcon, WM_TRAY};
use crate::ui::settings::{self, SettingsHost, SettingsResult};
use crate::ui::theme;
use crate::ui::weather_icons::{self, WeatherIcon};
use crate::ui::widget::{Widget, WM_WIDGET_CLICK, WM_WIDGET_FULLSCREEN, WM_WIDGET_MENU, WM_WIDGET_MOVED};
use crate::ui::window::{self, WindowHandler};
use crate::weather::{Client, WeatherInfo, weather_description};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, DispatchMessageW, GetMessageW, GetCursorPos,
    KillTimer, PostMessageW, PostQuitMessage, RegisterWindowMessageW, SetForegroundWindow,
    SetTimer, TranslateMessage, TrackPopupMenuEx, MENU_ITEM_FLAGS, MF_STRING,
    MSG, TPM_RIGHTBUTTON, TPM_VERTICAL, TPMPARAMS, WM_APP, WM_COMMAND,
    WM_DESTROY, WM_RBUTTONUP, WM_SETTINGCHANGE, WM_TIMER, WS_POPUP,
};
use crate::native::displays;

const TIMER_POS: usize = 1; // 30 s: reposition / fullscreen visibility, TaskbarPray pattern
const TIMER_WEATHER: usize = 2; // 5 min: refresh weather (cache governs actual fetch rate)

/// Weather fetch result; lparam = Box<WeatherInfo> raw.
const WM_WEATHER_RESULT: u32 = WM_APP + 5;

const CMD_FORCE_UPDATE: usize = 1001;
const CMD_SETTINGS: usize = 1002;
const CMD_EXIT: usize = 1003;

pub struct App {
    pub cfg: Settings,
    hwnd: HWND,
    widget: Option<Box<Widget>>,
    tray: Option<TrayIcon>,
    client: std::sync::Arc<Client>,
    text: String,
    code: i32,
    icon_kind: Option<WeatherIcon>,
    displayed_city: String,
    refreshing: bool,
    settings_open: bool,
    forecast: Option<Box<Forecast>>,
    taskbar_created_msg: u32,
}

impl App {
    pub fn run() {
        unsafe {
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let cfg = Settings::load();
        theme::apply(&cfg.theme, &cfg.font_family, cfg.font_scale());

        let app = Box::new(Self {
            cfg,
            hwnd: HWND::default(),
            widget: None,
            tray: None,
            client: std::sync::Arc::new(Client::new()),
            text: "Loading...".into(),
            code: -1,
            icon_kind: None,
            displayed_city: String::new(),
            refreshing: false,
            settings_open: false,
            forecast: None,
            taskbar_created_msg: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
        });
        let app = Box::leak(app); // lives for the process; freed by the OS at exit

        let hwnd = window::create(
            w!("WeatherPillApp"),
            w!("RustyWeatherPill"),
            Default::default(),
            WS_POPUP, // top-level, never shown
            0,
            0,
            0,
            0,
            None,
            app as *mut Self,
        )
          .expect("app window");
            app.hwnd = hwnd;
            app.tray = Some(TrayIcon::new(hwnd));
            if let Some(t) = &mut app.tray {
                // C# NotifyIcon Text = "WeatherPill": tooltip + the tray icon's accessible name.
                t.set_tooltip("RustyWeatherPill");
            }
        app.build_widget();
        app.fetch_weather(false);

        unsafe {
            SetTimer(Some(hwnd), TIMER_POS, 30_000, None);
            SetTimer(Some(hwnd), TIMER_WEATHER, 5 * 60 * 1000, None);
        }

        // Message loop.
        let mut msg = MSG::default();
        unsafe {
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn build_widget(&mut self) {
        self.widget = None; // drop the old window/hooks first
        self.widget = Some(Widget::new(self.hwnd, &self.cfg));
    }

    /// Spawn a synchronous fetch on a worker thread; the result is posted back.
    /// 5-minute ticks rely on the 45-minute cache, matching the C# refresh timer.
    /// Forecast popup toggle (C# ToggleForecast): open above the pill or close the open one.
    fn toggle_forecast(&mut self) {
        if self.settings_open {
            return;
        }
        if let Some(f) = &mut self.forecast {
            if f.alive() {
                f.close_graph(); // WM_GRAPH_SIZE cleanup lands right after
                self.forecast = None;
                return;
            }
            self.forecast = None;
        }
        let (gw, gh) = self.cfg.graph_size();
        let desc = weather_description(self.code);
        let f = Forecast::new(
            self.hwnd,
            self.widget.as_ref().map(|w| w.hwnd).unwrap_or_default(),
            &self.cfg.city,
            &desc,
            self.client.clone(),
            gw,
            gh,
        );
        self.forecast = Some(f);
    }

    fn fetch_weather(&mut self, force: bool) {
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        let client = self.client.clone();
        let city = self.cfg.city.clone();
        let target = self.hwnd.0 as isize;
        std::thread::spawn(move || {
            let mut info = client.get_weather(&city, force);
            // One-shot hourly read (not a poll) so the tray tooltip can show the current
            // precipitation chance alongside type and temperature.
            info.precip_pct = client.get_hourly(&city).first().map(|h| h.precip_pct).unwrap_or(0);
            let info = Box::new(info);
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(target as *mut _)),
                    WM_WEATHER_RESULT,
                    WPARAM(0),
                    LPARAM(Box::into_raw(info) as isize),
                );
            }
        });
    }

    /// Port of Overlay.RefreshWeatherAsync: swap in fresh text; on failure keep the last good
    /// reading unless nothing has been shown yet or the city changed underneath us.
    fn on_weather_result(&mut self, info: WeatherInfo) {
        self.refreshing = false;
        if info.ok() || self.text == "Loading..." || self.displayed_city != self.cfg.city {
            self.text = info.text.clone();
            self.code = info.code;
            self.displayed_city = self.cfg.city.clone();
        }
        if let Some(w) = &mut self.widget {
            w.set_weather(&self.text, self.code);
        }
        self.sync_tray_icon();
        self.sync_tray_tooltip(&info);
    }

    /// Swap the tray icon when the weather condition actually changed. Driven by the
    /// existing weather-refresh path (no extra timer/poll): the WMO code is bucketed to
    /// one of six icons and compared with the one currently applied.
    fn sync_tray_icon(&mut self) {
        let Some(kind) = weather_icons::for_code(self.code) else { return };
        if self.icon_kind == Some(kind) {
            return;
        }
        if let Some(tray) = &mut self.tray {
            if let Some(icon) = weather_icons::load(kind) {
                tray.set_icon(icon);
                self.icon_kind = Some(kind);
            }
        }
    }

    /// Update the tray tooltip on hover: weather type, temperature, precipitation chance.
    fn sync_tray_tooltip(&mut self, info: &WeatherInfo) {
        if !info.ok() {
            return;
        }
        let desc = weather_description(info.code);
        let text = format!("{} {}°C, {}% precip", desc, info.temp_c, info.precip_pct);
        if let Some(tray) = &mut self.tray {
            tray.set_tooltip(&text);
        }
    }

    /// Weather text now: forced fetch (Force Update) or cached timing tick.
    fn refresh_tick(&mut self, force: bool) {
        self.fetch_weather(force);
    }

    fn pos_tick(&mut self) {
        if let Some(w) = &mut self.widget {
            w.tick();
        }
    }

    /// The pill's right-click and the tray icon's click both open this menu
    /// (C# ContextMenuStrip: Force Update / Settings / Exit).
    fn show_menu(&mut self) {
        // A modal dialog runs its own message loop that still dispatches tray clicks; suppress
        // the menu while settings are open (TaskbarPray AppHost.showMenu).
        if self.settings_open {
            return;
        }
        unsafe {
            let Ok(menu) = CreatePopupMenu() else { return };
            let add = |flags: MENU_ITEM_FLAGS, id: usize, text: &str| {
                let wide = window::utf16z(text);
                let _ = AppendMenuW(menu, flags, id, windows::core::PCWSTR(wide.as_ptr()));
            };
            add(MF_STRING, CMD_FORCE_UPDATE, "Force Update");
            add(MF_STRING, CMD_SETTINGS, "Settings");
            add(MF_STRING, CMD_EXIT, "Exit");

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            // Required for the menu to dismiss when clicking elsewhere (classic tray-menu quirk).
            let _ = SetForegroundWindow(self.hwnd);
            // Exclude the taskbar strip: menus clip to the monitor (not work area), and the
            // taskbar z-band would hide any rows placed under it. Both the tray icon and the
            // pill live inside that strip, so the cursor point itself must be excluded.
            let point_rect = RECT { left: pt.x, top: pt.y, right: pt.x, bottom: pt.y };
            let exclude = displays::at_point(pt)
                .map(|m| {
                    let (b, w) = (m.bounds, m.work);
                    if w.bottom < b.bottom {
                        RECT { left: b.left, top: w.bottom, right: b.right, bottom: b.bottom }
                    } else if w.top > b.top {
                        RECT { left: b.left, top: b.top, right: b.right, bottom: w.top }
                    } else if w.left > b.left {
                        RECT { left: b.left, top: b.top, right: w.left, bottom: b.bottom }
                    } else {
                        point_rect
                    }
                })
                .unwrap_or(point_rect);
            let params = TPMPARAMS {
                cbSize: std::mem::size_of::<TPMPARAMS>() as u32,
                rcExclude: exclude,
            };
            let _ = TrackPopupMenuEx(
                menu,
                TPM_RIGHTBUTTON.0 | TPM_VERTICAL.0,
                pt.x,
                pt.y,
                self.hwnd,
                Some(&params),
            );
            let _ = DestroyMenu(menu);
        }
    }

    fn command(&mut self, id: usize) {
        match id {
            CMD_FORCE_UPDATE => self.refresh_tick(true),
            CMD_SETTINGS => self.run_settings(),
            CMD_EXIT => self.exit(),
            _ => {}
        }
    }

    /// Modal settings (AppHost.RunSettings pattern): pause the widget + timers, run the
    /// nested loop, then re-apply and rearm. A changed city/emoji set re-fetches forced.
    fn run_settings(&mut self) {
        if self.settings_open {
            return;
        }
        self.settings_open = true;
        if let Some(w) = &mut self.widget {
            w.pause();
        }
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_POS);
            let _ = KillTimer(Some(self.hwnd), TIMER_WEATHER);
        }
        let orig = self.cfg.clone();
        let result = settings::run(self);
        if result == SettingsResult::Ok {
            let city_changed = self.cfg.city != orig.city;
            self.apply_config();
            if city_changed {
                self.fetch_weather(true);
            }
        }
        unsafe {
            SetTimer(Some(self.hwnd), TIMER_POS, 30_000, None);
            SetTimer(Some(self.hwnd), TIMER_WEATHER, 5 * 60 * 1000, None);
        }
        if let Some(w) = &mut self.widget {
            w.resume();
        }
        self.pos_tick();
        self.settings_open = false;
    }

    /// Re-apply theme + widget layout/style from the (possibly mutated) cfg without raising
    /// windows — TaskbarPray AppHost.LivePreview, minus the prayer recompute.
    fn apply_config(&mut self) {
        theme::apply(&self.cfg.theme, &self.cfg.font_family, self.cfg.font_scale());
        let current = self
            .widget
            .as_ref()
            .and_then(|w| w.device_name())
            .unwrap_or("")
            .to_string();
        let wanted = self.cfg.monitor_device_name.clone().unwrap_or_default();
        if !current.eq_ignore_ascii_case(&wanted) {
            self.build_widget();
            return;
        }
        if let Some(w) = &mut self.widget {
            w.apply_layout(&self.cfg);
        }
        self.pos_tick();
    }

    #[allow(dead_code)] // wired to the Exit menu command above
    fn exit(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_POS);
            let _ = KillTimer(Some(self.hwnd), TIMER_WEATHER);
        }
        self.forecast = None; // Widget/Forecast Drop destroys the windows
        self.widget = None;
        if let Some(tray) = &mut self.tray {
            tray.remove();
        }
        unsafe { PostQuitMessage(0) };
    }
}

impl SettingsHost for App {
    fn cfg(&mut self) -> &mut Settings {
        &mut self.cfg
    }

    /// Re-apply the (mutated-in-place) config without raising windows.
    fn apply(&mut self) {
        self.apply_config();
    }
}

impl WindowHandler for App {
    fn message(&mut self, _hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_TIMER => {
                match wparam.0 {
                    TIMER_POS => self.pos_tick(),
                    TIMER_WEATHER => self.refresh_tick(false),
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_WEATHER_RESULT => {
                let info = *unsafe { Box::from_raw(lparam.0 as *mut WeatherInfo) };
                self.on_weather_result(info);
                Some(LRESULT(0))
            }
            WM_WIDGET_CLICK => {
                self.toggle_forecast();
                Some(LRESULT(0))
            }
            WM_FORECAST_RESULT => {
                // The widget died before the worker finished: free the payload, skip the paint.
                let hours = unsafe { Box::from_raw(lparam.0 as *mut Vec<crate::weather::HourlyEntry>) };
                if let Some(f) = &mut self.forecast {
                    if f.alive() {
                        f.set_hours(*hours);
                    }
                }
                Some(LRESULT(0))
            }
            WM_GRAPH_SIZE => {
                let w = (lparam.0 & 0xFFFF_FFFF) as i32;
                let h = (lparam.0 >> 32) as i32;
                if w >= 320 && h >= 190 && (self.cfg.graph_w != w || self.cfg.graph_h != h) {
                    self.cfg.graph_w = w;
                    self.cfg.graph_h = h;
                    self.cfg.save();
                }
                self.forecast = None;
                Some(LRESULT(0))
            }
            WM_WIDGET_MENU => {
                self.show_menu();
                Some(LRESULT(0))
            }
            WM_TRAY => {
                // Classic NotifyIcon callback: lParam is the mouse message. The menu is a
                // right-click action only — hover/moves (WM_MOUSEMOVE) must not open it.
                if lparam.0 as u32 == WM_RBUTTONUP as u32 {
                    self.show_menu();
                }
                Some(LRESULT(0))
            }
            WM_COMMAND => {
                self.command(wparam.0 & 0xFFFF);
                Some(LRESULT(0))
            }
            WM_WIDGET_MOVED => {
                self.cfg.widget_offset = (wparam.0 as i32).clamp(0, 2000);
                self.cfg.sanitize();
                self.cfg.save();
                Some(LRESULT(0))
            }
            WM_WIDGET_FULLSCREEN => {
                // The pill hides itself behind a fullscreen app; close the forecast popup
                // too so it can't stay above the fullscreen layer.
                if let Some(f) = &mut self.forecast {
                    if f.alive() {
                        f.close_graph();
                    }
                }
                self.forecast = None;
                Some(LRESULT(0))
            }
            WM_SETTINGCHANGE => {
                // Follow Windows' light/dark flip only when the user left the theme on "Auto".
                if lparam.0 != 0 {
                    let s = unsafe { windows::core::PCWSTR(lparam.0 as *const u16).to_string() }
                        .unwrap_or_default();
                    if s == "ImmersiveColorSet" && self.cfg.theme.eq_ignore_ascii_case("Auto") {
                        theme::apply(&self.cfg.theme, &self.cfg.font_family, self.cfg.font_scale());
                        if let Some(w) = &mut self.widget {
                            w.rerender();
                        }
                    }
                }
                None
            }
            m if m == self.taskbar_created_msg && m != 0 => {
                if let Some(tray) = &mut self.tray {
                    tray.readd();
                }
                self.build_widget(); // Explorer restarted -> old owner window is gone
                self.refresh_tick(false);
                Some(LRESULT(0))
            }
            WM_DESTROY => unsafe {
                PostQuitMessage(0);
                Some(LRESULT(0))
            },
            _ => None,
        }
    }
}

