//! Themed settings dialog, port of WeatherPill/SettingsForm.cs (General/Position/Appearance
//! tabs flattened into one page) on the TaskbarPray dialog foundation: Save persists through
//! the host, Cancel reverts to the opening snapshot.

use crate::config::Settings;
use crate::native::displays;
use crate::ui::controls::{self, ButtonKind};
use crate::ui::theme;
use crate::ui::window::{self, WindowHandler};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{DeleteObject, HFONT, HDC};
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, DestroyWindow, DispatchMessageW, GetMessageW, IsDialogMessageW, MessageBoxW,
    MoveWindow, PostQuitMessage, SetForegroundWindow, ShowWindow, TranslateMessage,
    MB_ICONWARNING, MB_OK, MESSAGEBOX_STYLE, MSG, SW_SHOW, WM_CLOSE,
    WM_COMMAND, WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC,
    WM_DRAWITEM, WM_ERASEBKGND, WS_CAPTION, WS_EX_DLGMODALFRAME, WS_EX_TOPMOST,
    WS_SYSMENU,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsResult {
    Ok,
    Cancel,
}

/// What the dialog needs from the app; App implements it.
pub trait SettingsHost {
    fn cfg(&mut self) -> &mut Settings;
    /// Re-apply theme/widget config from the (mutated-in-place) cfg, without raising windows.
    fn apply(&mut self);
}

// Layout: compact, balanced settings page with consistent alignment and proportions.
// Dimensions are logical 96-DPI units; Dialog::s() applies the monitor DPI scale on top.
const M: i32 = 24;
const CTRL_H: i32 = 34;
const ROW_H: i32 = 50;
const SECTION_H: i32 = 22;
const LBL_X: i32 = M;
const LABEL_W: i32 = 176;
const CTRL_X: i32 = LBL_X + LABEL_W + 8; // 208
const CTRL_W: i32 = 328;
const SMALL_W: i32 = 328;
const BTN_W: i32 = 128;
const BTN_H: i32 = 36;
const CLIENT_W: i32 = 560;
const CLIENT_H: i32 = 580;

const ID_SAVE: i32 = 1001;
const ID_CANCEL: i32 = 1002;
const ID_CITY: i32 = 1003;
const ID_OFFSET_X: i32 = 1004;
const ID_OFFSET_Y: i32 = 1005;
const ID_FONT: i32 = 1006;
const ID_FONT_SIZE: i32 = 1007;
const ID_GRAPH: i32 = 1009;
const ID_UI_SMALL: i32 = 1011;
const ID_UI_MEDIUM: i32 = 1012;
const ID_UI_LARGE: i32 = 1013;

const GRAPH_SCALES: [f32; 7] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];
const UI_SIZES: [&str; 3] = ["Small", "Medium", "Large"];

struct Dialog {
    host: *mut dyn SettingsHost,
    hwnd: HWND,
    result: Option<SettingsResult>,
    scale: f32,
    brushes: controls::Brushes,
    font: HFONT,
    fonts: Vec<String>,
    ui_size_sel: usize,
}

/// Modal settings dialog on the current thread (nested message loop).
pub fn run(host: &mut dyn SettingsHost) -> SettingsResult {
    // Erase the borrow lifetime; the nested modal loop below keeps `host` alive and on-thread.
    let host_ptr: *mut dyn SettingsHost = unsafe { std::mem::transmute(host) };

    // Dialog font size from the user's UI size preference (separate from pill text size).
    let font_pt = unsafe {
        match (*host_ptr).cfg().ui_size.as_str() {
            "Small" => 12.0,
            "Large" => 16.0,
            _ => 14.0,
        }
    };

    let mut dlg = Box::new(Dialog {
        host: host_ptr,
        hwnd: HWND::default(),
        result: None,
        scale: 1.0,
        brushes: controls::Brushes::new(),
        font: HFONT::default(),
        fonts: controls::font_families(),
        ui_size_sel: 1,
    });

    let hwnd = window::create(
        w!("WeatherPillSettings"),
        w!("WeatherPill Settings"),
        WS_EX_DLGMODALFRAME | WS_EX_TOPMOST,
        WS_CAPTION | WS_SYSMENU,
        i32::MIN,
        i32::MIN,
        200,
        200,
        None,
        dlg.as_mut() as *mut Dialog,
    )
    .expect("settings window");
    dlg.hwnd = hwnd;

    if theme::current().is_dark {
        let dark: i32 = 1;
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark as *const _ as *const core::ffi::c_void,
                4,
            );
        }
    }

    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let dpi_scale = if dpi > 0 { dpi as f32 / 96.0 } else { 1.0 };
    dlg.scale = dpi_scale;
    dlg.font = controls::ui_font(&theme::family(), font_pt, false, dpi.max(96));
    dlg.place_window();
    dlg.build();
    dlg.load_values();
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }

    let mut msg = MSG::default();
    unsafe {
        while dlg.result.is_none() {
            if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                PostQuitMessage(msg.wParam.0 as i32); // re-post so the outer loop exits too
                break;
            }
            if IsDialogMessageW(dlg.hwnd, &msg).as_bool() {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    dlg.result.take().unwrap_or(SettingsResult::Cancel)
}

impl Drop for Dialog {
    fn drop(&mut self) {
        unsafe {
            if !self.font.is_invalid() {
                let _ = DeleteObject(self.font.into());
            }
        }
    }
}

impl Dialog {
    fn host(&mut self) -> &mut dyn SettingsHost {
        unsafe { &mut *self.host }
    }

    fn s(&self, v: i32) -> i32 {
        (v as f32 * self.scale).round() as i32
    }

    fn place_window(&self) {
        let style = WS_CAPTION | WS_SYSMENU;
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: self.s(CLIENT_W),
            bottom: self.s(CLIENT_H),
        };
        unsafe {
            let _ = AdjustWindowRectEx(&mut rc, style, false, WS_EX_DLGMODALFRAME);
        }
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        let work = displays::primary().map(|m| m.work).unwrap_or(RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        });
        let x = work.left + ((work.right - work.left) - w) / 2;
        let y = work.top + ((work.bottom - work.top) - h) / 2;
        unsafe {
            let _ = MoveWindow(self.hwnd, x, y.max(work.top), w, h, false);
        }
    }

    // --- construction ---

    fn lbl(&mut self, text: &str, x: i32, y: i32, w: i32, dim: bool) -> HWND {
        let h = controls::label(self.hwnd, text, self.s(x), self.s(y) + 2, self.s(w), self.s(30));
        controls::set_font(h, self.font);
        let _ = dim;
        h
    }

    fn section_lbl(&mut self, text: &str, x: i32, y: i32, w: i32) -> HWND {
        let h = controls::label(self.hwnd, text, self.s(x), self.s(y), self.s(w), self.s(SECTION_H));
        controls::set_font(h, self.font);
        h
    }

    fn edit_at(&mut self, id: i32, text: &str, x: i32, y: i32, w: i32) -> HWND {
        let h = controls::edit(self.hwnd, text, self.s(x), self.s(y), self.s(w), self.s(CTRL_H), id);
        controls::set_font(h, self.font);
        h
    }

    fn combo_at(&mut self, id: i32, x: i32, y: i32, w: i32) -> HWND {
        let h = controls::combo(self.hwnd, self.s(x), self.s(y), self.s(w), self.s(400), id);
        controls::set_font(h, self.font);
        controls::combo_set_item_height(h, self.s(CTRL_H));
        h
    }

    fn btn_at(&mut self, id: i32, text: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
        let b = controls::button(self.hwnd, text, self.s(x), self.s(y), self.s(w), self.s(h), id);
        controls::set_font(b, self.font);
        b
    }

    fn build(&mut self) {
        let mut y = M;

        // --- Location ---
        self.section_lbl("LOCATION", LBL_X, y, CLIENT_W - 2 * M);
        y += self.s(SECTION_H);
        self.lbl("City", LBL_X, y, LABEL_W, false);
        self.edit_at(ID_CITY, "", CTRL_X, y - 2, CTRL_W);
        y += ROW_H;

        // --- Widget Position ---
        self.section_lbl("WIDGET POSITION", LBL_X, y, CLIENT_W - 2 * M);
        y += self.s(SECTION_H);
        self.lbl("Horizontal offset", LBL_X, y, LABEL_W, false);
        self.edit_at(ID_OFFSET_X, "", CTRL_X, y - 2, CTRL_W);
        y += ROW_H;
        self.lbl("Vertical offset", LBL_X, y, LABEL_W, false);
        self.edit_at(ID_OFFSET_Y, "", CTRL_X, y - 2, CTRL_W);
        y += ROW_H;

        // --- Appearance ---
        self.section_lbl("APPEARANCE", LBL_X, y, CLIENT_W - 2 * M);
        y += self.s(SECTION_H);
        self.lbl("Font family", LBL_X, y, LABEL_W, false);
        let fonts = self.combo_at(ID_FONT, CTRL_X, y - 2, CTRL_W);
        for f in self.fonts.clone() {
            controls::combo_add(fonts, &f);
        }
        controls::combo_set_dropped_width(fonts, 600);
        y += ROW_H;

        self.lbl("Font size (pt)", LBL_X, y, LABEL_W, false);
        self.edit_at(ID_FONT_SIZE, "", CTRL_X, y - 2, SMALL_W);
        y += ROW_H;

        self.lbl("Graph scale", LBL_X, y, LABEL_W, false);
        let graph = self.combo_at(ID_GRAPH, CTRL_X, y - 2, SMALL_W);
        for g in GRAPH_SCALES {
            controls::combo_add(graph, &format!("{:.2}x", g));
        }
        controls::combo_set_dropped_width(graph, 220);
        y += ROW_H;

        self.lbl("Interface size", LBL_X, y, LABEL_W, false);
        let bw = 96;
        self.btn_at(ID_UI_SMALL, "Small", CTRL_X, y, bw, CTRL_H);
        self.btn_at(ID_UI_MEDIUM, "Medium", CTRL_X + bw + 8, y, bw, CTRL_H);
        self.btn_at(ID_UI_LARGE, "Large", CTRL_X + 2 * (bw + 8), y, bw, CTRL_H);
        y += ROW_H;

        // Keep the usage hint visually separate from editable settings.
        self.lbl(
            "Tip: Drag the pill to reposition it. Left-click opens the forecast.",
            LBL_X,
            y + 2,
            CLIENT_W - 2 * M,
            true,
        );
        y += ROW_H;

        // Actions share a baseline and sit flush to the lower-right edge.
        self.btn_at(ID_CANCEL, "Cancel", CLIENT_W - M - 2 * BTN_W - 12, y - 2, BTN_W, BTN_H);
        self.btn_at(ID_SAVE, "Save changes", CLIENT_W - M - BTN_W, y - 2, BTN_W, BTN_H);
    }

    fn load_values(&mut self) {
        let cfg = self.host().cfg().clone();
        let item = |id: i32| -> HWND {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetDlgItem(Some(self.hwnd), id)
                    .unwrap_or_default()
            }
        };
        controls::set_text(item(ID_CITY), &cfg.city);
        controls::set_text(item(ID_OFFSET_X), &cfg.widget_offset.to_string());
        controls::set_text(item(ID_OFFSET_Y), &cfg.offset_y.to_string());
        controls::set_text(item(ID_FONT_SIZE), &format!("{:.1}", cfg.font_size));
        let font_sel = index_of(&self.fonts, &cfg.font_family)
            .or_else(|| index_of(&self.fonts, "Segoe UI"))
            .unwrap_or(0);
        controls::combo_set(item(ID_FONT), font_sel as i32);
        let graph_i = nearest_scale(cfg.graph_w as f32);
        controls::combo_set(item(ID_GRAPH), graph_i as i32);
        let ui_i = UI_SIZES
            .iter()
            .position(|s| s.eq_ignore_ascii_case(&cfg.ui_size))
            .unwrap_or(1);
        self.ui_size_sel = ui_i;
    }

    // --- commit ---

    fn save(&mut self) {
        let city = controls::get_text(self.item(ID_CITY)).trim().to_string();
        if city.is_empty() {
            self.msg("City is required.", "WeatherPill", MB_ICONWARNING);
            return;
        }

        // Read every control first, then commit to the host config once.
        let widget_offset = controls::get_text(self.item(ID_OFFSET_X))
            .trim()
            .parse()
            .unwrap_or(self.host().cfg().widget_offset);
        let offset_y = controls::get_text(self.item(ID_OFFSET_Y))
            .trim()
            .parse()
            .unwrap_or(self.host().cfg().offset_y);
        let mut font_family = self.host().cfg().font_family.clone();
        let font_sel = controls::combo_sel(self.item(ID_FONT));
        if font_sel >= 0 {
            let family = combo_text(self.item(ID_FONT), font_sel as u32);
            if !family.is_empty() {
                font_family = family;
            }
        }
        let font_size = controls::get_text(self.item(ID_FONT_SIZE))
            .trim()
            .parse::<f64>()
            .unwrap_or(self.host().cfg().font_size)
            .clamp(6.0, 36.0);
        let mut graph_w = 0;
        let mut graph_h = 0;
        let graph_i = controls::combo_sel(self.item(ID_GRAPH));
        if (0..GRAPH_SCALES.len() as i32).contains(&graph_i) {
            let scale = GRAPH_SCALES[graph_i as usize];
            graph_w = (480.0 * scale).round() as i32;
            graph_h = (240.0 * scale).round() as i32;
        }
        let ui_size = UI_SIZES[self.ui_size_sel].to_string();

        // Also refresh the font family's textual fallback when the combo lists it oddly.
        let cfg = self.host().cfg();
        cfg.city = city;
        cfg.widget_offset = widget_offset;
        cfg.offset_y = offset_y;
        cfg.font_family = font_family;
        cfg.font_size = font_size;
        cfg.ui_size = ui_size;
        cfg.graph_w = graph_w;
        cfg.graph_h = graph_h;
        cfg.sanitize();
        self.host().cfg().clone().save();
        self.finish(SettingsResult::Ok);
    }

    fn item(&self, id: i32) -> HWND {
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetDlgItem(Some(self.hwnd), id).unwrap_or_default()
        }
    }

    fn msg(&self, body: &str, title: &str, icon: MESSAGEBOX_STYLE) {
        unsafe {
            let b = window::utf16z(body);
            let t = window::utf16z(title);
            let _ = MessageBoxW(
                Some(self.hwnd),
                windows::core::PCWSTR(b.as_ptr()),
                windows::core::PCWSTR(t.as_ptr()),
                icon | MB_OK,
            );
        }
    }

    fn finish(&mut self, result: SettingsResult) {
        if self.result.is_none() {
            self.result = Some(result);
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

fn index_of(list: &[String], needle: &str) -> Option<usize> {
    list.iter().position(|n| n.eq_ignore_ascii_case(needle))
}

fn nearest_scale(graph_w: f32) -> usize {
    let scale = graph_w / 480.0;
    let mut best = 0;
    for (i, s) in GRAPH_SCALES.iter().enumerate() {
        if (s - scale).abs() < (GRAPH_SCALES[best] - scale).abs() {
            best = i;
        }
    }
    best
}

impl WindowHandler for Dialog {
    fn message(&mut self, _hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_ERASEBKGND => {
                let mut rc = RECT::default();
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(self.hwnd, &mut rc);
                    let hdc = HDC(wparam.0 as *mut _);
                    let brush = windows::Win32::Graphics::Gdi::CreateSolidBrush(
                        ctl_color_of(theme::current().panel),
                    );
                    windows::Win32::Graphics::Gdi::FillRect(hdc, &rc, brush);
                    let _ = windows::Win32::Graphics::Gdi::DeleteObject(brush.into());
                }
                Some(LRESULT(1))
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                let p = theme::current();
                let hdc = HDC(wparam.0 as *mut _);
                let brush = controls::ctl_colors(hdc, p.text, p.panel, self.brushes.panel);
                Some(LRESULT(brush.0 as isize))
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                let p = theme::current();
                let hdc = HDC(wparam.0 as *mut _);
                let brush = controls::ctl_colors(hdc, p.text, p.bg_hover, self.brushes.field);
                Some(LRESULT(brush.0 as isize))
            }
            WM_DRAWITEM => {
                const ODT_COMBOBOX: u32 = 3; // winuser.h; not surfaced by the windows crate
                let dis = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
                if dis.CtlType.0 == ODT_COMBOBOX {
                    controls::draw_combo(dis, self.font, false);
                    return Some(LRESULT(1));
                }
                let id = dis.CtlID as i32;
                let kind = if id == ID_SAVE {
                    ButtonKind::Accent
                } else if id == ID_UI_SMALL || id == ID_UI_MEDIUM || id == ID_UI_LARGE {
                    let idx = match id {
                        ID_UI_SMALL => 0,
                        ID_UI_MEDIUM => 1,
                        _ => 2,
                    };
                    if idx == self.ui_size_sel {
                        ButtonKind::Accent
                    } else {
                        ButtonKind::Normal
                    }
                } else {
                    ButtonKind::Normal
                };
                let text = controls::get_text(dis.hwndItem);
                controls::draw_button(dis, &text, self.font, kind, false);
                Some(LRESULT(1))
            }
            WM_COMMAND => {
                let id = (wparam.0 & 0xFFFF) as i32;
                match id {
                    ID_SAVE => self.save(),
                    ID_CANCEL => self.finish(SettingsResult::Cancel),
                    ID_UI_SMALL => {
                        self.ui_size_sel = 0;
                        for i in [ID_UI_SMALL, ID_UI_MEDIUM, ID_UI_LARGE] {
                            unsafe {
                                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.item(i)), None, false);
                            }
                        }
                    }
                    ID_UI_MEDIUM => {
                        self.ui_size_sel = 1;
                        for i in [ID_UI_SMALL, ID_UI_MEDIUM, ID_UI_LARGE] {
                            unsafe {
                                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.item(i)), None, false);
                            }
                        }
                    }
                    ID_UI_LARGE => {
                        self.ui_size_sel = 2;
                        for i in [ID_UI_SMALL, ID_UI_MEDIUM, ID_UI_LARGE] {
                            unsafe {
                                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.item(i)), None, false);
                            }
                        }
                    }
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                self.finish(SettingsResult::Cancel);
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}

fn ctl_color_of(argb: u32) -> windows::Win32::Foundation::COLORREF {
    controls::colorref(argb)
}

fn combo_text(h: HWND, idx: u32) -> String {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{CB_GETLBTEXT, CB_GETLBTEXTLEN, SendMessageW};
        let len = SendMessageW(h, CB_GETLBTEXTLEN, Some(WPARAM(idx as usize)), None).0;
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = SendMessageW(h, CB_GETLBTEXT, Some(WPARAM(idx as usize)), Some(LPARAM(buf.as_mut_ptr() as isize))).0;
        if n <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..n as usize])
    }
}
