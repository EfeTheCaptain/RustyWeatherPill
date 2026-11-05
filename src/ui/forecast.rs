//! Forecast popup, a 1:1 port of the C# ForecastForm: GDI+ temp curve + precip bars + resize grip.
//! Persistent toggle: the popup only closes via explicit paths (pill toggle, click inside, Esc) —
//! never through focus loss. All drawing math mirrors DrawGraph exactly.

use crate::native::{displays, taskbar};
use crate::ui::gdip::{self, Bitmap, Font, Graphics, SolidBrush, StringFormat};
use crate::ui::window::{self, WindowHandler};
use crate::weather::{Client, HourlyEntry};
use std::sync::Arc;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT, ScreenToClient,
};
use windows::Win32::Graphics::GdiPlus::PointF;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, GetCursorPos, IsWindow, LoadCursorW, PostMessageW,
    SendMessageW, SetCursor, SetForegroundWindow, SetLayeredWindowAttributes, ShowWindow,
    HTBOTTOMRIGHT, HTCLIENT, IDC_ARROW, IDC_SIZENWSE, LWA_ALPHA, MINMAXINFO, SW_SHOW,
    WM_CLOSE, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_NCLBUTTONDOWN, WM_PAINT, WM_SETCURSOR, WM_SIZE,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WM_APP,
};
use windows::core::w;

/// Posted to the app window when the popup closes; w/h ride packed in lparam (C# SaveGraphSize).
pub const WM_GRAPH_SIZE: u32 = WM_APP + 11;
/// Posted by the hourly-fetch worker; a Box<Vec<HourlyEntry>> rides in lparam.
pub const WM_FORECAST_RESULT: u32 = WM_APP + 12;

const PRECIP_BAND_HEIGHT: i32 = 30;
const LABEL_HEIGHT: i32 = 16;
const GRIP_SIZE_PX: i32 = 26;
const MIN_W: i32 = 320;
const MIN_H: i32 = 190;

// Fixed dark palette — the C# popup is intentionally not theme-driven.
const BG: u32 = 0xFF_18_18_1C;
const GRID: u32 = 0xFF_2D_2D_34;
const AXIS: u32 = 0xFF_46_46_4E;
const LABEL_COLOR: u32 = 0xFF_8C_8C_92;
const TEMP_LINE: u32 = 0xFF_78_BE_FF;
const PRECIP_TEXT: u32 = 0xFF_6E_AA_FF;
const POINT_COLOR: u32 = 0xFF_D7_E6_FA;

pub struct Forecast {
    pub hwnd: HWND,
    app_hwnd: HWND,
    owner_hwnd: HWND,
    city: String,
    weather_desc: String,
    hours: Vec<HourlyEntry>,
    status: String,
    down_in_grip: bool,
    font_header: Font,
    font_label: Font,
    font_status: Font,
    font_precip: Font,
    fmt: StringFormat,
    dpi: f32,
    buffer: Option<Bitmap>,
    measure_bmp: Bitmap,
}

impl Forecast {
    pub fn new(
        app_hwnd: HWND,
        owner_hwnd: HWND,
        city: &str,
        weather_desc: &str,
        client: Arc<Client>,
        w: i32,
        h: i32,
    ) -> Box<Self> {
        let mut boxx = Box::new(Self {
            hwnd: HWND::default(),
            app_hwnd,
            owner_hwnd,
            city: city.to_string(),
            weather_desc: weather_desc.to_string(),
            hours: Vec::new(),
            status: "Loading forecast...".into(),
            down_in_grip: false,
            font_header: Font::new_pt("Segoe UI", 8.5, gdip::STYLE_BOLD),
            font_label: Font::new_pt("Segoe UI", 7.5, gdip::STYLE_REGULAR),
            font_status: Font::new_pt("Segoe UI", 9.0, gdip::STYLE_REGULAR),
            font_precip: Font::new_pt("Segoe UI", 7.0, gdip::STYLE_REGULAR),
            fmt: StringFormat::new(gdip::ALIGN_NEAR, gdip::ALIGN_NEAR),
            dpi: 96.0,
            buffer: None,
            measure_bmp: Bitmap::new(1, 1),
        });

        let (x, y) = boxx.place_above_owner(w, h);
        let hwnd = window::create(
            w!("WeatherPillForecast"),
            w!(""),
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            boxx.as_mut() as *mut Self,
        )
        .expect("forecast window");
        boxx.hwnd = hwnd;
        unsafe {
            // Form.Opacity = 0.95
            let _ = SetLayeredWindowAttributes(hwnd, windows::Win32::Foundation::COLORREF(0), 242, LWA_ALPHA);
            boxx.dpi = GetDpiForWindow(hwnd) as f32;
            let _ = ShowWindow(hwnd, SW_SHOW);
            // Form.Activate(); foreground-lock-aware best effort, same as C#.
            let _ = SetForegroundWindow(hwnd);
        }
        boxx.render_frame();

        // Deferred fetch, WeatherService.GetHourlyAsync shaped onto a worker thread.
        let target = app_hwnd.0 as isize;
        let city = boxx.city.clone();
        std::thread::spawn(move || {
            let hours = Box::new(client.get_hourly(&city));
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(target as *mut _)),
                    WM_FORECAST_RESULT,
                    WPARAM(0),
                    LPARAM(Box::into_raw(hours) as isize),
                );
            }
        });
        boxx
    }

    /// The window can die under us only via DestroyWindow from the app's cleanup path.
    pub fn alive(&self) -> bool {
        !self.hwnd.is_invalid() && unsafe { IsWindow(Some(self.hwnd)) }.as_bool()
    }

    pub fn set_hours(&mut self, hours: Vec<HourlyEntry>) {
        self.hours = hours;
        if self.hours.is_empty() {
            self.status = "No forecast available".into();
        }
        self.render_frame();
        self.invalidate();
    }

    /// C# PlaceAboveOwner: above the pill inside the pill monitor's working area.
    /// Returns the initial position; never re-run while open (C# does it once in OnLoad).
    fn place_above_owner(&self, w: i32, h: i32) -> (i32, i32) {
        let r = taskbar::window_rect(self.owner_hwnd).unwrap_or_default();
        let screen = displays::from_window(self.owner_hwnd);
        let wa = screen.map(|m| m.work).unwrap_or(RECT::default());

        let x = (r.left).clamp(wa.left, (wa.right - w).max(wa.left));
        let y = r.top - h - 4;
        let y = if y < wa.top { r.bottom + 4 } else { y };
        let y = y.clamp(wa.top, (wa.bottom - h).max(wa.top));
        (x, y)
    }

    fn in_grip(&self, x: f32, y: f32) -> bool {
        let cw = self.client_w() as f32;
        let ch = self.client_h() as f32;
        x >= cw - GRIP_SIZE_PX as f32 && y >= ch - GRIP_SIZE_PX as f32 && x > 0.0 && y > 0.0
    }

    fn client_rect(&self) -> RECT {
        taskbar::window_rect(self.hwnd).unwrap_or_default()
    }

    fn client_w(&self) -> i32 {
        self.client_rect().right - self.client_rect().left
    }

    fn client_h(&self) -> i32 {
        self.client_rect().bottom - self.client_rect().top
    }

    /// Close paths converge here: persist the size (C# FormClosing -> SaveGraphSize), die.
    pub fn close_graph(&mut self) {
        let r = self.client_rect();
        let w = r.right - r.left;
        let h = r.bottom - r.top;
        if w >= MIN_W && h >= MIN_H {
            let packed = ((h as isize) << 32) | (w as isize & 0xFFFF_FFFF);
            unsafe {
                let _ = PostMessageW(
                    Some(self.app_hwnd),
                    WM_GRAPH_SIZE,
                    WPARAM(0),
                    LPARAM(packed),
                );
            }
        }
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }

    fn invalidate(&self) {
        if !self.hwnd.is_invalid() {
            unsafe {
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        }
    }

    fn render_frame(&mut self) {
        let bmp = Bitmap::new(self.client_w().max(1), self.client_h().max(1));
        {
            let g = Graphics::from_bitmap(&bmp);
            g.clear(BG);
            if self.hours.is_empty() {
                let status = self.status.clone();
                let (mw, mh) = Graphics::from_bitmap(&self.measure_bmp).measure_string(&status, &self.font_status);
                let cw = self.client_w() as f32;
                let ch = self.client_h() as f32;
                let brush = SolidBrush::new(LABEL_COLOR);
                g.draw_string(
                    &status,
                    &self.font_status,
                    &brush,
                    gdip::rectf((cw - mw) / 2.0, (ch - mh) / 2.0, mw + 2.0, mh),
                    &self.fmt,
                );
            } else {
                self.draw_graph(&g);
                self.draw_grip(&g);
            }
        }
        self.buffer = Some(bmp);
    }

    fn paint(&mut self) {
        let mut ps = PAINTSTRUCT::default();
        let hdc = unsafe { BeginPaint(self.hwnd, &mut ps) };
        if let Some(buf) = &self.buffer {
            let g = Graphics::from_hdc(hdc);
            g.draw_bitmap(buf, 0, 0);
        }
        unsafe {
            let _ = EndPaint(self.hwnd, &ps);
        }
    }

    fn draw_grip(&self, g: &Graphics) {
        let cw = self.client_w();
        let ch = self.client_h();
        let pen = gdip::Pen::new(0xFF_5A_5A_60, 1.0);
        for i in 0..4 {
            let off = 5 + i * 4;
            g.draw_line(&pen, (cw - off) as f32, (ch - 4) as f32, (cw - 4) as f32, (ch - off) as f32);
        }
    }

    /// Port of DrawGraph — every constant and formula matches the C#.
    fn draw_graph(&self, g: &Graphics) {
        let cw = self.client_w() as f32;
        let ch = self.client_h() as f32;
        let pad_left = 34.0;
        let pad_right = 14.0;

        let precip_top = ch - LABEL_HEIGHT as f32 - PRECIP_BAND_HEIGHT as f32;
        let plot_top = 22.0;
        let plot_bottom = precip_top - 8.0;
        let (plot_l, plot_t, plot_w, plot_h) = (pad_left, plot_top, cw - pad_left - pad_right, plot_bottom - plot_top);

        let measure = Graphics::from_bitmap(&self.measure_bmp);
        let n = self.hours.len();
        let mut xs = vec![0.0f32; n];
        let spacing = plot_w / ((n.saturating_sub(1)).max(1) as f32);

        let mut t_min = f64::MAX;
        let mut t_max = f64::MIN;
        for (i, e) in self.hours.iter().enumerate() {
            xs[i] = plot_l + i as f32 * spacing;
            if e.temp_c < t_min {
                t_min = e.temp_c;
            }
            if e.temp_c > t_max {
                t_max = e.temp_c;
            }
        }
        if t_max - t_min < 1.0 {
            let mid = (t_max + t_min) / 2.0;
            t_min = mid - 0.5;
            t_max = mid + 0.5;
        }
        t_min -= 1.0;
        t_max += 1.0;
        let y_of = |t: f64| plot_t + plot_h - (((t - t_min) / (t_max - t_min) * plot_h as f64) as f32);

        // header
        let header_brush = SolidBrush::new(LABEL_COLOR);
        let header = format!("{} — Next 12 hours - {}", self.weather_desc, self.city);
        let header_fmt = StringFormat::new(gdip::ALIGN_NEAR, gdip::ALIGN_CENTER);
        g.draw_string(&header, &self.font_header, &header_brush, gdip::rectf(plot_l, 5.0, plot_w, 20.0), &header_fmt);
        let (uw, uh) = measure.measure_string("°C", &self.font_label);
        g.draw_string("°C", &self.font_label, &header_brush, gdip::rectf(cw - uw - 8.0, 6.0, uw + 2.0, uh), &self.fmt);

        // horizontal grid + Y labels
        let grid_pen = gdip::Pen::new(GRID, 1.0);
        let label_brush = SolidBrush::new(LABEL_COLOR);
        for i in 0..=4 {
            let value = t_min + (t_max - t_min) * i as f64 / 4.0;
            let y = y_of(value).round();
            g.draw_line(&grid_pen, plot_l, y, plot_l + plot_w, y);
            let text = format!("{}°", value.round());
            let (tw, th) = measure.measure_string(&text, &self.font_label);
            g.draw_string(&text, &self.font_label, &label_brush, gdip::rectf(3.0, y - 9.0, tw + 2.0, th), &self.fmt);
        }

        // subtle area fill under the curve
        let mut polygon: Vec<PointF> = Vec::with_capacity(n + 2);
        for (i, e) in self.hours.iter().enumerate() {
            polygon.push(PointF { X: xs[i], Y: y_of(e.temp_c) });
        }
        polygon.push(PointF { X: xs[n - 1], Y: plot_bottom });
        polygon.push(PointF { X: xs[0], Y: plot_bottom });
        g.fill_polygon(&SolidBrush::new(0x1E_00_00_00 | (TEMP_LINE & 0x00FF_FFFF)), &polygon);

        // temperature line
        let mut line_pts: Vec<PointF> = Vec::with_capacity(n);
        for (i, e) in self.hours.iter().enumerate() {
            line_pts.push(PointF { X: xs[i], Y: y_of(e.temp_c) });
        }
        let temp_pen = gdip::Pen::new(TEMP_LINE, 2.0);
        g.set_pen_join_round(&temp_pen);
        g.draw_lines(&temp_pen, &line_pts);

        // data points + min/max annotations
        let point_brush = SolidBrush::new(POINT_COLOR);
        let annotation_brush = SolidBrush::new(0xFFFFFFFF);
        let (i_max, i_min) = {
            let mut i_max = 0usize;
            let mut i_min = 0usize;
            for i in 1..n {
                if self.hours[i].temp_c > self.hours[i_max].temp_c {
                    i_max = i;
                }
                if self.hours[i].temp_c < self.hours[i_min].temp_c {
                    i_min = i;
                }
            }
            (i_max, i_min)
        };
        for i in 0..n {
            let px = xs[i];
            let py = y_of(self.hours[i].temp_c);
            g.fill_ellipse(&point_brush, px - 2.5, py - 2.5, 5.0, 5.0);
            if i == i_max || i == i_min {
                let text = format!("{}°", self.hours[i].temp_c.round());
                let (tw, th) = measure.measure_string(&text, &self.font_label);
                g.draw_string(&text, &self.font_label, &annotation_brush, gdip::rectf(px - tw / 2.0, py - 16.0, tw + 2.0, th), &self.fmt);
            }
        }

        // x axis separator
        let axis_pen = gdip::Pen::new(AXIS, 1.0);
        g.draw_line(&axis_pen, plot_l, plot_bottom, plot_l + plot_w, plot_bottom);

        // precipitation band
        let baseline = precip_top + PRECIP_BAND_HEIGHT as f32 - 4.0;
        let mut bar_max_h = (plot_bottom + 14.0) - baseline;
        if bar_max_h < 4.0 {
            bar_max_h = 4.0;
        }
        let bar_w = 9.0f32.min(spacing * 0.45);
        g.draw_line(&axis_pen, plot_l, baseline, plot_l + plot_w, baseline);
        let pct_brush = SolidBrush::new(PRECIP_TEXT);
        for (i, e) in self.hours.iter().enumerate() {
            let pct = e.precip_pct.clamp(0, 100);
            if pct > 0 {
                let bar_h = (bar_max_h * pct as f32 / 100.0).round();
                let bar_brush = SolidBrush::new((((50 + pct) as u32) << 24) | (90 << 16) | (160 << 8) | 255);
                g.fill_rect(&bar_brush, xs[i] - bar_w / 2.0, baseline - bar_h, bar_w, bar_h);
            }
            let text = format!("{}%", pct);
            let (tw, th) = measure.measure_string(&text, &self.font_precip);
            g.draw_string(&text, &self.font_precip, &pct_brush, gdip::rectf(xs[i] - tw / 2.0, plot_bottom + 3.0, tw + 2.0, th), &self.fmt);
        }

        // hour labels
        for (i, e) in self.hours.iter().enumerate() {
            let text = e.label();
            let (tw, th) = measure.measure_string(&text, &self.font_label);
            g.draw_string(&text, &self.font_label, &label_brush, gdip::rectf(xs[i] - tw / 2.0, ch - LABEL_HEIGHT as f32 + 1.0, tw + 2.0, th), &self.fmt);
        }
    }
}

impl WindowHandler for Forecast {
    fn message(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_ERASEBKGND => Some(LRESULT(1)),
            WM_PAINT => {
                self.paint();
                Some(LRESULT(0))
            }
            WM_SIZE => {
                self.render_frame();
                self.invalidate();
                None
            }
            WM_GETMINMAXINFO => {
                let mmi = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
                let sf = self.dpi / 96.0;
                mmi.ptMinTrackSize.x = (MIN_W as f32 * sf).round() as i32;
                mmi.ptMinTrackSize.y = (MIN_H as f32 * sf).round() as i32;
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                if wparam.0 as u32 == VK_ESCAPE.0 as u32 {
                    self.close_graph();
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
                self.down_in_grip = self.in_grip(x as f32, y as f32);
                if self.down_in_grip {
                    let _ = unsafe { ReleaseCapture() };
                    unsafe {
                        SendMessageW(hwnd, WM_NCLBUTTONDOWN, Some(WPARAM(HTBOTTOMRIGHT as usize)), Some(LPARAM(0)));
                    }
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                if !self.down_in_grip {
                    self.close_graph();
                }
                self.down_in_grip = false;
                Some(LRESULT(0))
            }
            WM_SETCURSOR => {
                let hit = (lparam.0 & 0xFFFF) as u32;
                if hit == HTCLIENT {
                    let mut pt = Default::default();
                    let _ = unsafe { GetCursorPos(&mut pt) };
                    let _ = unsafe { ScreenToClient(hwnd, &mut pt) };
                    let cursor = if self.in_grip(pt.x as f32, pt.y as f32) {
                        unsafe { LoadCursorW(None, IDC_SIZENWSE).unwrap() }
                    } else {
                        unsafe { LoadCursorW(None, IDC_ARROW).unwrap() }
                    };
                    unsafe { SetCursor(Some(cursor)) };
                    Some(LRESULT(0))
                } else {
                    None
                }
            }
            WM_CLOSE => {
                // C# FormClosing persists size first (SaveGraphSize), then closes.
                self.close_graph();
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}

impl Drop for Forecast {
    fn drop(&mut self) {
        if unsafe { IsWindow(Some(self.hwnd)) }.as_bool() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}
