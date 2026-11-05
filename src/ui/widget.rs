//! The taskbar weather pill: WeatherPill's Overlay ported onto the TaskbarPray widget shell.
//! A top-level always-on-top overlay owned by the taskbar window so it rides the taskbar's
//! z-band; content is rendered from a weather text that mixes temperature runs and emoji runs.

use crate::config::{self, Settings};
use crate::native::{displays, taskbar};
use crate::ui::gdip::{self, Bitmap, Font, Graphics, SolidBrush, StringFormat};
use crate::ui::theme;
use crate::ui::window::{self, WindowHandler};
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, InvalidateRect, SetWindowRgn};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_LBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, GetCursorPos, IsWindowVisible, MoveWindow, PostMessageW, ShowWindow,
    EVENT_OBJECT_REORDER, EVENT_SYSTEM_FOREGROUND, SW_HIDE, SW_SHOWNA, WINEVENT_OUTOFCONTEXT,
    WM_APP, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_PAINT,
    WM_RBUTTONUP, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_CAPTURECHANGED: u32 = 0x0215;

/// Posted to the app window on pill left-click (toggle the forecast popup).
pub const WM_WIDGET_CLICK: u32 = WM_APP + 2;
/// Posted to the app window on pill right-click; cursor pos packed in lparam.
pub const WM_WIDGET_MENU: u32 = WM_APP + 3;
/// Posted to the app window after a drag; the new logical offset rides in wparam.
pub const WM_WIDGET_MOVED: u32 = WM_APP + 9;
/// Posted to the widget window by the win-event hook (raise/visibility check).
const WM_RAISE: u32 = WM_APP + 10;

const MA_NOACTIVATE: u32 = 3;

/// Pixels of travel before a press counts as a drag rather than a click (C# DragThresholdPx).
const DRAG_SLOP: i32 = 4;

// Hook callbacks carry no context; a single widget exists at a time.
static HOOK_TARGET: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    let target = HOOK_TARGET.load(Ordering::Relaxed);
    if target != 0 {
        let _ = unsafe { PostMessageW(Some(HWND(target as *mut _)), WM_RAISE, WPARAM(0), LPARAM(0)) };
    }
}

/// Emoji-vs-text run splitting, port of Overlay.SplitRuns. Variation selectors / ZWJ are
/// implied by the emoji font; drawing them shows tofu. In char space the C# surrogate-lead
/// test becomes the U+1F000+ plane range.
fn is_emoji_char(c: char) -> bool {
    let v = c as u32;
    (0x2600..=0x27BF).contains(&v)     // misc symbols: ☀ ⛅ ❄ ⛈
        || (0x2B00..=0x2BFF).contains(&v) // arrows/stars
        || (0x1F000..=0x1FFFF).contains(&v) // U+1F000+ (= the C# surrogate-pair lead test)
}

fn split_runs(text: &str) -> Vec<(bool, String)> {
    let mut runs: Vec<(bool, String)> = Vec::new();
    let mut cur = String::new();
    let mut cur_emoji = false;
    for c in text.chars() {
        if c == '\u{FE0F}' || c == '\u{200D}' {
            continue;
        }
        let emoji = is_emoji_char(c);
        if !cur.is_empty() && emoji != cur_emoji {
            runs.push((cur_emoji, std::mem::take(&mut cur)));
        }
        if cur.is_empty() {
            cur_emoji = emoji;
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        runs.push((cur_emoji, cur));
    }
    runs
}

/// Hidden state that rides with the widget but comes from the settings dialog.
#[derive(Clone)]
pub struct Style {
    pub family: String,
    pub font_pt: f32,
    pub font_scale: f32,
}

pub struct Widget {
    pub hwnd: HWND,
    app_hwnd: HWND,
    device: Option<String>,
    pub anchor_right: bool,
    pub offset: i32,
    pub offset_y: i32,
    pub hide_on_fullscreen: bool,
    style: Style,
    text: String,
    code: i32,
    drag_from: Option<(i32, i32)>, // (cursor x, window x) at mouse-down; Some() = pressed
    drag_moved: bool,              // travelled past the slop, so this is a drag not a click
    hover: bool,
    tracking: bool,
    paused: bool,
    suppressed: bool,
    w: i32,
    h: i32,
    run_h: f32, // underlying (unscaled) measured line height for the current text
    scale: f32,
    last_rect: RECT,
    buffer: Option<Bitmap>,
    measure_bmp: Bitmap,
    fg_hook: HWINEVENTHOOK,
    reorder_hook: HWINEVENTHOOK,
    last_raise: u32,
}

impl Style {
    pub fn new(cfg: &Settings) -> Self {
        Self {
            family: cfg.font_family.clone(),
            font_pt: cfg.font_pt(),
            font_scale: cfg.font_scale(),
        }
    }
}

impl Widget {
    pub fn new(app_hwnd: HWND, cfg: &Settings) -> Box<Self> {
        gdip::init();
        let device = cfg.monitor_device_name.clone().filter(|d| !d.is_empty());
        let tb = taskbar::taskbar_for_device(device.as_deref());
        let scale = tb.map(taskbar::scale_of).unwrap_or(1.0);

        let mut widget = Box::new(Self {
            hwnd: HWND::default(),
            app_hwnd,
            device,
            anchor_right: !cfg.widget_anchor.eq_ignore_ascii_case("Left"),
            offset: cfg.widget_offset,
            offset_y: cfg.offset_y,
            hide_on_fullscreen: true,
            style: Style::new(cfg),
            text: "Loading...".into(),
            code: -1,
            drag_from: None,
            drag_moved: false,
            hover: false,
            tracking: false,
            paused: false,
            suppressed: false,
            w: 150,
            h: 28,
            run_h: 16.0,
            scale: if scale > 0.0 { scale } else { 1.0 },
            last_rect: RECT::default(),
            buffer: None,
            measure_bmp: Bitmap::new(1, 1),
            fg_hook: HWINEVENTHOOK::default(),
            reorder_hook: HWINEVENTHOOK::default(),
            last_raise: 0,
        });

        let hwnd = window::create(
            w!("WeatherPillWidget"),
            w!(""),
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            WS_POPUP | WS_VISIBLE,
            0,
            0,
            widget.w,
            widget.h,
            tb, // owner -> rides in the taskbar's z-band
            widget.as_mut() as *mut Self,
        )
        .expect("widget window");
        widget.hwnd = hwnd;
        HOOK_TARGET.store(hwnd.0 as isize, Ordering::Relaxed);

        // Re-raise the instant the taskbar comes forward. Foreground hook is global (infrequent);
        // the chatty reorder hook is scoped to the taskbar's thread — never system-wide.
        unsafe {
            widget.fg_hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            if let Some(tb) = tb {
                let tid = taskbar::thread_of(tb);
                if tid != 0 {
                    widget.reorder_hook = SetWinEventHook(
                        EVENT_OBJECT_REORDER,
                        EVENT_OBJECT_REORDER,
                        None,
                        Some(win_event_proc),
                        0,
                        tid,
                        WINEVENT_OUTOFCONTEXT,
                    );
                }
            }
        }

        widget.resize_to_content();
        widget.render_buffer();
        widget.sync_position();
        widget
    }

    pub fn device_name(&self) -> Option<&str> {
        self.device.as_deref()
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    /// New weather: text like "21°C ☀" + code; resize, repaint, move.
    pub fn set_weather(&mut self, text: &str, code: i32) {
        if self.text == text && self.code == code {
            return;
        }
        self.text = text.into();
        self.code = code;
        self.resize_to_content();
        self.render_buffer();
        self.invalidate();
    }

    pub fn apply_style(&mut self, style: &Style) {
        self.style = style.clone();
        self.resize_to_content();
        self.render_buffer();
        self.invalidate();
    }

    /// Settings Save/Init: re-anchor and restyle from config, then reposition (never raises).
    pub fn apply_layout(&mut self, cfg: &Settings) {
        self.anchor_right = !cfg.widget_anchor.eq_ignore_ascii_case("Left");
        self.offset = cfg.widget_offset;
        self.offset_y = cfg.offset_y;
        self.style = Style::new(cfg);
        self.resize_to_content();
        self.render_buffer();
        self.invalidate();
    }

    fn main_font(&self) -> Font {
        // Settings font is point-based (C# new Font(family, pt, bold)); convert to px at 96 DPI,
        // then apply the taskbar DPI scale and the user's extra font-scale percentage.
        Font::new(&self.style.family, self.style.font_px() * self.scale, gdip::STYLE_BOLD)
    }

    fn emoji_font(&self) -> Font {
        let pt = (self.style.font_pt * config::EMOJI_FONT_RATIO).max(7.0);
        Font::new("Segoe UI Emoji", pt * 4.0 / 3.0 * self.style.font_scale * self.scale, gdip::STYLE_BOLD)
    }

    fn measure(&self, s: &str, font: &Font) -> f32 {
        Graphics::from_bitmap(&self.measure_bmp).measure_string(s, font).0
    }

    /// Content-fit width: env padding + each run's width; height from the tallest run.
    fn resize_to_content(&mut self) {
        let f_main = self.main_font();
        let f_emoji = self.emoji_font();
        let mut w = config::PAD_X * 2.0;
        let mut h = 0.0f32;
        for (emoji, run) in split_runs(&self.text) {
            let f = if emoji { &f_emoji } else { &f_main };
            let m = Graphics::from_bitmap(&self.measure_bmp).measure_string(&run, f);
            w += m.0.ceil();
            h = h.max(m.1.ceil());
        }
        self.run_h = h;
        self.w = self.s(w).max(self.s(config::PILL_MIN_W as f32));
        self.h = (self.s(h)).max(self.s(config::PILL_MIN_H as f32)) + self.s(config::PAD_Y * 2.0);
    }

    /// 1 s safety tick; hooks handle the instant cases.
    pub fn tick(&mut self) {
        self.update_visibility();
    }

    fn update_visibility(&mut self) {
        if self.hwnd.is_invalid() || self.paused {
            return;
        }
        if self.hide_on_fullscreen {
            if let Some(m) = displays::by_device(self.device.as_deref()) {
                if taskbar::is_fullscreen_app_on(&m.bounds) {
                    if !self.suppressed {
                        self.suppressed = true;
                        self.hide();
                    }
                    return;
                }
            }
        }
        if self.suppressed {
            self.suppressed = false;
            self.show();
        }
        self.sync_position();
    }

    pub fn screen_rect(&self) -> RECT {
        taskbar::window_rect(self.hwnd).unwrap_or_default()
    }

    pub fn visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd) }.as_bool()
    }

    pub fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNA);
        }
    }

    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// Stop fighting modal dialogs: a topmost re-raise dismisses an open combo dropdown.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
        self.update_visibility();
    }

    pub fn rerender(&mut self) {
        self.resize_to_content();
        self.render_buffer();
        self.invalidate();
    }

    pub fn sync_position(&mut self) {
        if !self.paused {
            self.position_core(true);
        }
    }

    /// Settings live preview: bypasses the pause guard and never re-raises.
    pub fn preview_reposition(&mut self) {
        self.position_core(false);
    }

    /// The band the pill lives in: (strip rect, right edge, left edge). The right edge stops at
    /// the tray cluster so the pill never lands under the clock.
    fn strip(&self) -> Option<(RECT, i32, i32)> {
        let screen = displays::by_device(self.device.as_deref())?;
        let tb = taskbar::taskbar_for_device(self.device.as_deref());
        let on_its_bar = tb
            .and_then(displays::from_window)
            .map(|m| m.device.eq_ignore_ascii_case(&screen.device))
            .unwrap_or(false);
        if on_its_bar {
            let r = taskbar::window_rect(tb?)?;
            if r.right <= r.left {
                return None;
            }
            let tray_left = taskbar::tray_notify_left(tb?);
            Some((r, if tray_left > 0 { tray_left } else { r.right }, r.left))
        } else {
            // No taskbar on this monitor -> float at its bottom.
            let b = screen.bounds;
            let h = self.s(40.0);
            let r = RECT { left: b.left, top: b.bottom - h, right: b.right, bottom: b.bottom };
            Some((r, b.right, b.left))
        }
    }

    fn drag_to(&mut self, cursor_x: i32) {
        let Some((start_cursor, start_window)) = self.drag_from else { return };
        // A lost button-up (capture stolen, released off the pill) would otherwise leave the
        // pill following a button-less cursor on every hover.
        if unsafe { GetKeyState(VK_LBUTTON.0 as i32) } >= 0 {
            self.end_drag(true);
            return;
        }
        if !self.drag_moved && (cursor_x - start_cursor).abs() < DRAG_SLOP {
            return; // still within slop: this may yet be a plain click
        }
        self.drag_moved = true;
        let Some((_, right_edge, left_edge)) = self.strip() else { return };
        // widget_offset is clamped to 0..2000 logical units on save, so anything dropped beyond
        // that would spring back on the next reposition. Stop the pill there instead.
        let span = (2000.0 * self.scale) as i32;
        let (lo, hi) = if self.anchor_right {
            ((right_edge - self.w - span).max(left_edge), right_edge - self.w)
        } else {
            (left_edge, (left_edge + span).min(right_edge - self.w))
        };
        let x = (start_window + cursor_x - start_cursor).clamp(lo.min(hi), hi.max(lo));
        // Drag is horizontal-only (WeatherPill behavior); keep the current Y.
        let y = self.last_rect.top;
        unsafe {
            let _ = MoveWindow(self.hwnd, x, y, self.w, self.h, true);
        }
        // position_core early-returns when its target equals last_rect, so it has to learn
        // where the window actually went or the next tick would skip the correcting move.
        self.last_rect = RECT { left: x, top: y, right: x + self.w, bottom: y + self.h };
    }

    /// Release the drag. `commit` persists where it landed; a cancelled drag leaves the offset
    /// alone and lets the next reposition put the pill back.
    fn end_drag(&mut self, commit: bool) {
        let was_pressed = self.drag_from.take().is_some();
        let moved = self.drag_moved;
        self.drag_moved = false;
        if was_pressed {
            unsafe {
                let _ = ReleaseCapture();
            }
        }
        if commit && moved {
            self.commit_drag();
        }
    }

    /// Turn the dragged position back into the anchored, DPI-independent offset config stores.
    fn commit_drag(&mut self) {
        let Some((_, right_edge, left_edge)) = self.strip() else { return };
        let px = if self.anchor_right {
            right_edge - self.last_rect.left - self.w
        } else {
            self.last_rect.left - left_edge
        };
        self.offset = ((px as f32 / self.scale).round() as i32).clamp(0, 2000);
        unsafe {
            let _ = PostMessageW(
                Some(self.app_hwnd),
                WM_WIDGET_MOVED,
                WPARAM(self.offset as usize),
                LPARAM(0),
            );
        }
    }

    fn position_core(&mut self, raise: bool) {
        if self.drag_from.is_some() {
            return; // the pointer owns the position until the button comes up
        }
        let Some(screen) = displays::by_device(self.device.as_deref()) else { return };
        let tb = taskbar::taskbar_for_device(self.device.as_deref());
        let on_its_bar = tb
            .and_then(displays::from_window)
            .map(|m| m.device.eq_ignore_ascii_case(&screen.device))
            .unwrap_or(false);

        let prev_scale = self.scale;
        let scale_src = if on_its_bar { tb.unwrap() } else { self.hwnd };
        self.scale = taskbar::scale_of(scale_src);
        if self.scale <= 0.0 {
            self.scale = 1.0;
        }
        if self.scale != prev_scale {
            self.resize_to_content();
            self.render_buffer();
            self.invalidate();
        }

        let Some((strip, right_edge, left_edge)) = self.strip() else { return };

        // Y: offset_y = distance from the top edge of the taskbar, clamped inside (C# model).
        let y = (strip.top + self.s(self.offset_y as f32))
            .clamp(strip.top, (strip.bottom - self.h).max(strip.top));

        // X: anchored to the tray edge plus the DPI-independent widget_offset.
        let x = if self.anchor_right {
            right_edge - self.w - self.s(self.offset as f32)
        } else {
            left_edge + self.s(self.offset as f32)
        };

        let target = RECT { left: x, top: y, right: x + self.w, bottom: y + self.h };
        if target == self.last_rect {
            if raise {
                taskbar::raise_topmost(self.hwnd);
            }
            return;
        }
        self.last_rect = target;
        unsafe {
            let _ = MoveWindow(self.hwnd, x, y, self.w, self.h, true);
        }
        if raise {
            taskbar::raise_topmost(self.hwnd);
        }
        self.update_region();
    }

    fn update_region(&self) {
        let r = self.s(8.0).min(self.h / 2);
        unsafe {
            let rgn = CreateRoundRectRgn(0, 0, self.w + 1, self.h + 1, r * 2, r * 2);
            let _ = SetWindowRgn(self.hwnd, Some(rgn), true); // window takes ownership
        }
    }

    fn invalidate(&self) {
        if !self.hwnd.is_invalid() {
            unsafe {
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        }
    }

    fn render_buffer(&mut self) {
        let pal = theme::current();
        let bmp = Bitmap::new(self.w.max(1), self.h.max(1));
        {
            let g = Graphics::from_bitmap(&bmp);
            g.clear(if self.hover { pal.bg_hover } else { pal.bg });

            let runs = split_runs(&self.text);
            let mut x = self.s(config::PAD_X) as f32;
            let text_brush = SolidBrush::new(pal.text);
            let emoji_brush = SolidBrush::new(theme::emoji_color(self.code));
            let center = StringFormat::new(gdip::ALIGN_NEAR, gdip::ALIGN_CENTER);
            for (emoji, run) in runs {
                let f = if emoji { self.emoji_font() } else { self.main_font() };
                let wpx = self.measure(&run, &f);
                if emoji {
                    // The brush paint makes monochrome-font emoji tinted; bright standard-set
                    // glyphs read well on both pill bases.
                    g.draw_string(
                        &run,
                        &f,
                        &emoji_brush,
                        gdip::rectf(x, 0.0, wpx, self.h as f32),
                        &center,
                    );
                } else {
                    g.draw_string(
                        &run,
                        &f,
                        &text_brush,
                        gdip::rectf(x, 0.0, wpx + self.s(2.0) as f32, self.h as f32),
                        &center,
                    );
                }
                x += wpx;
            }
        }
        self.buffer = Some(bmp);
    }

    fn paint(&mut self) {
        use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
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

    fn track_leave(&self) {
        let mut tme = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: self.hwnd,
            dwHoverTime: 0,
        };
        unsafe {
            let _ = TrackMouseEvent(&mut tme);
        }
    }
}

impl Style {
    /// Point-size fonts behave as px at true 96 DPI graphics; keep the unchanged pt value
    /// available for scaling math.
    fn font_px(&self) -> f32 {
        self.font_pt * 4.0 / 3.0 * self.font_scale
    }
}

impl WindowHandler for Widget {
    fn message(&mut self, _hwnd: HWND, msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_ERASEBKGND => Some(LRESULT(1)),
            WM_MOUSEACTIVATE => Some(LRESULT(MA_NOACTIVATE as isize)),
            WM_PAINT => {
                self.paint();
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                let mut pt = Default::default();
                if unsafe { GetCursorPos(&mut pt) }.is_ok() {
                    unsafe { SetCapture(self.hwnd) };
                    self.drag_from = Some((pt.x, self.last_rect.left));
                    self.drag_moved = false;
                }
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE => {
                if self.drag_from.is_some() {
                    let mut pt = Default::default();
                    if unsafe { GetCursorPos(&mut pt) }.is_ok() {
                        self.drag_to(pt.x);
                    }
                }
                if !self.tracking {
                    self.track_leave();
                    self.tracking = true;
                }
                if !self.hover {
                    self.hover = true;
                    self.render_buffer();
                    self.invalidate();
                }
                None
            }
            WM_MOUSELEAVE => {
                self.tracking = false;
                if self.hover {
                    self.hover = false;
                    self.render_buffer();
                    self.invalidate();
                }
                None
            }
            WM_LBUTTONUP => {
                let click = self.drag_from.is_some() && !self.drag_moved;
                self.end_drag(true);
                if click {
                    unsafe {
                        let _ = PostMessageW(Some(self.app_hwnd), WM_WIDGET_CLICK, WPARAM(0), LPARAM(0));
                    }
                }
                Some(LRESULT(0))
            }
            // Capture can go without a button-up: hiding on fullscreen mid-drag, a menu taking it,
            // the secure desktop. Without this the drag state sticks and position_core is dead.
            WM_CAPTURECHANGED => {
                self.drag_from = None;
                self.drag_moved = false;
                None
            }
            WM_RBUTTONUP => {
                let mut pt = Default::default();
                unsafe {
                    let _ = GetCursorPos(&mut pt);
                    let packed = ((pt.y as isize) << 32) | (pt.x as isize & 0xFFFF_FFFF);
                    let _ = PostMessageW(Some(self.app_hwnd), WM_WIDGET_MENU, WPARAM(0), LPARAM(packed));
                }
                None
            }
            WM_RAISE => {
                if !self.paused {
                    let now = tick_count();
                    if now.wrapping_sub(self.last_raise) >= 16 {
                        self.last_raise = now;
                        self.update_visibility();
                    }
                }
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}

fn tick_count() -> u32 {
    unsafe { windows::Win32::System::SystemInformation::GetTickCount() }
}

impl Drop for Widget {
    fn drop(&mut self) {
        HOOK_TARGET.store(0, Ordering::Relaxed);
        unsafe {
            if !self.fg_hook.is_invalid() {
                let _ = UnhookWinEvent(self.fg_hook);
            }
            if !self.reorder_hook.is_invalid() {
                let _ = UnhookWinEvent(self.reorder_hook);
            }
            if !self.hwnd.is_invalid() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_temperature_and_emoji_runs() {
        let runs = split_runs("21°C ☀️");
        // [text "21°C "][emoji "☀"] — VS16 skipped
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0], (false, "21°C ".to_string()));
        assert_eq!(runs[1], (true, "☀".to_string()));
    }

    #[test]
    fn minimalist_glyphs_stay_text_per_the_csharp_ranges() {
        // ≋ (U+224B) sits below U+2600, exactly like SplitRuns classified it.
        let runs = split_runs("≋");
        assert_eq!(runs, vec![(false, "≋".to_string())]);
    }

    #[test]
    fn plane1_emoji_are_recognized() {
        assert_eq!(split_runs("🌧️"), vec![(true, "🌧".to_string())]);
        assert_eq!(split_runs("🌡"), vec![(true, "🌡".to_string())]);
    }
}
