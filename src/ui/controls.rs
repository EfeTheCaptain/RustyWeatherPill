//! Themed native Win32 child controls (combo/edit/checkbox/owner-drawn buttons)
//! plus the WM_CTLCOLOR* brush set for the settings dialog. Port of TaskbarPray's
//! controls.rs, trimmed to what the WeatherPill-style dialog needs.
//! Backgrounds use GDI+ anti-aliased rounded rects where possible.
#![allow(dead_code)]

use crate::ui::theme::{self, mix};
use crate::ui::window;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EnumFontFamiliesExW,
    GetDC, InvalidateRect, Polyline, ReleaseDC, SelectObject, SetBkMode,
    SetTextColor, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_QUALITY, DT_CENTER, DT_END_ELLIPSIS,
    DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_BOLD, FW_NORMAL, HBRUSH, HDC,
    HFONT, LOGFONTW, OUT_DEFAULT_PRECIS, PS_SOLID, TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::UI::Controls::{
    SetWindowTheme, DRAWITEMSTRUCT, ODS_COMBOBOXEDIT, ODS_DISABLED, ODS_HOTLIGHT, ODS_SELECTED,
};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, SendMessageW,
    SetWindowLongPtrW, SetWindowTextW, BS_OWNERDRAW, CBS_DROPDOWNLIST, CBS_HASSTRINGS,
    CBS_OWNERDRAWFIXED, CB_ADDSTRING, CB_GETCURSEL, CB_GETLBTEXT, CB_GETLBTEXTLEN, CB_SETCURSEL,
    ES_AUTOHSCROLL, GWLP_USERDATA, HMENU, WINDOW_EX_STYLE, WINDOW_STYLE, WM_SETFONT, WS_BORDER,
    WS_CHILD, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};
use windows::Win32::Graphics::GdiPlus::{
    GdipAddPathArcI, GdipAddPathLineI, GdipClosePathFigure, GdipCreateFromHDC,
    GdipCreatePath, GdipCreateSolidFill, GdipDeleteBrush, GdipDeleteGraphics, GdipDeletePath,
    GdipFillPath, GdipSetSmoothingMode, FillModeWinding, GpBrush, GpGraphics, GpPath,
    GpSolidFill, SmoothingModeAntiAlias,
};

const EM_SETCUEBANNER: u32 = 0x1501;
const EM_SETREADONLY: u32 = 0x00CF;
const SS_LEFTNOWORDWRAP: u32 = 0x0000_000C;

/// 0xAARRGGBB (theme) -> 0x00BBGGRR (GDI).
pub fn colorref(argb: u32) -> COLORREF {
    COLORREF(((argb & 0xFF) << 16) | (argb & 0xFF00) | ((argb >> 16) & 0xFF))
}

/// Black or white, whichever reads on the accent (port of SettingsForm.OnAccent).
pub fn on_accent(accent: u32) -> u32 {
    let (r, g, b) = ((accent >> 16) & 0xFF, (accent >> 8) & 0xFF, accent & 0xFF);
    if (r * 299 + g * 587 + b * 114) / 1000 > 140 {
        0xFF00_0000
    } else {
        0xFFFF_FFFF
    }
}

fn solid(argb: u32) -> HBRUSH {
    unsafe { CreateSolidBrush(colorref(argb)) }
}

/// Anti-aliased rounded-rect fill via GDI+.
/// `color` is 0xAARRGGBB. `radius` in physical pixels (already DPI-scaled by caller).
/// If radius*2 >= w or radius*2 >= h, draws a full pill/stadium shape.
pub unsafe fn fill_round_rect(
    hdc: HDC,
    x: i32, y: i32, w: i32, h: i32,
    radius: i32,
    color: u32,
) {
    let mut graphics: *mut GpGraphics = std::ptr::null_mut();
    let mut path: *mut GpPath = std::ptr::null_mut();
    let mut brush: *mut GpSolidFill = std::ptr::null_mut();

    // Create graphics from HDC
    if GdipCreateFromHDC(hdc, &mut graphics).0 != 0 {
        return;
    }
    GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);

    // Create path
    if GdipCreatePath(FillModeWinding, &mut path).0 != 0 {
        GdipDeleteGraphics(graphics);
        return;
    }

    let d = radius * 2;

    if d >= w || d >= h {
        // Full pill / stadium: two semicircles + connecting lines
        GdipAddPathArcI(path, x, y, d, h, 90.0, 180.0);
        GdipAddPathLineI(path, x, y + h, x + w - d, y + h);
        GdipAddPathArcI(path, x + w - d, y, d, h, 270.0, 180.0);
        GdipAddPathLineI(path, x + w, y + h / 2, x, y + h / 2);
    } else {
        // Standard rounded rect: 4 corner arcs + 4 connecting lines
        // Top-left corner + top edge
        GdipAddPathArcI(path, x, y, d, d, 180.0, 90.0);
        GdipAddPathLineI(path, x + d, y, x + w - d, y);
        // Top-right corner + right edge
        GdipAddPathArcI(path, x + w - d, y, d, d, 270.0, 90.0);
        GdipAddPathLineI(path, x + w, y + d, x + w, y + h - d);
        // Bottom-right corner + bottom edge
        GdipAddPathArcI(path, x + w - d, y + h - d, d, d, 0.0, 90.0);
        GdipAddPathLineI(path, x + w - d, y + h, x + d, y + h);
        // Bottom-left corner + left edge
        GdipAddPathArcI(path, x, y + h - d, d, d, 90.0, 90.0);
        GdipAddPathLineI(path, x, y + h - d, x, y + d);
    }
    GdipClosePathFigure(path);

    // Create brush and fill
    if GdipCreateSolidFill(color, &mut brush).0 != 0 {
        GdipDeletePath(path);
        GdipDeleteGraphics(graphics);
        return;
    }

    GdipFillPath(graphics, brush as *mut GpBrush, path);

    // Cleanup
    GdipDeleteBrush(brush as *mut GpBrush);
    GdipDeletePath(path);
    GdipDeleteGraphics(graphics);
}

/// Cached WM_CTLCOLOR* brushes for the current palette.
pub struct Brushes {
    pub bg: HBRUSH,
    pub panel: HBRUSH,
    pub field: HBRUSH,
}

impl Brushes {
    pub fn new() -> Self {
        let p = theme::current();
        Self {
            bg: solid(p.bg),
            panel: solid(p.panel),
            field: solid(p.bg_hover),
        }
    }
}

impl Default for Brushes {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Brushes {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.bg.into());
            let _ = DeleteObject(self.panel.into());
            let _ = DeleteObject(self.field.into());
        }
    }
}

pub fn ui_font(family: &str, pt: f32, bold: bool, italic: bool, dpi: u32) -> HFONT {
    let name = window::utf16z(family);
    let height = -((pt * dpi as f32 / 72.0).round() as i32);
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            if bold { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
            if italic { 1 } else { 0 },
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            DEFAULT_QUALITY,
            FF_DONTCARE.0 as u32,
            PCWSTR(name.as_ptr()),
        )
    }
}

fn create(parent: HWND, class: PCWSTR, text: &str, style: u32, x: i32, y: i32, w: i32, h: i32, id: i32) -> HWND {
    let t = window::utf16z(text);
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            PCWSTR(t.as_ptr()),
            WINDOW_STYLE(style) | WS_CHILD | WS_VISIBLE,
            x,
            y,
            w,
            h,
            Some(parent),
            Some(HMENU(id as usize as *mut core::ffi::c_void)),
            None,
            None,
        )
        .unwrap_or_default()
    }
}

pub fn label(parent: HWND, text: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
    create(parent, w!("STATIC"), text, SS_LEFTNOWORDWRAP, x, y, w, h, 0)
}

pub fn edit(parent: HWND, text: &str, x: i32, y: i32, w: i32, h: i32, id: i32) -> HWND {
    create(
        parent,
        w!("EDIT"),
        text,
        WS_BORDER.0 | WS_TABSTOP.0 | ES_AUTOHSCROLL as u32,
        x,
        y,
        w,
        h,
        id,
    )
}

pub fn combo(parent: HWND, x: i32, y: i32, w: i32, h: i32, id: i32) -> HWND {
    let c = create(
        parent,
        w!("COMBOBOX"),
        "",
        WS_TABSTOP.0 | WS_VSCROLL.0 | (CBS_DROPDOWNLIST | CBS_HASSTRINGS | CBS_OWNERDRAWFIXED) as u32,
        x,
        y,
        w,
        h,
        id,
    );
    if theme::current().is_dark {
        unsafe {
            let _ = SetWindowTheme(c, w!("DarkMode_CFD"), PCWSTR::null());
        }
    }
    c
}

pub fn set_font(h: HWND, f: HFONT) {
    unsafe {
        SendMessageW(h, WM_SETFONT, Some(WPARAM(f.0 as usize)), Some(LPARAM(1)));
    }
}

pub fn combo_add(h: HWND, s: &str) {
    let t = window::utf16z(s);
    unsafe {
        SendMessageW(h, CB_ADDSTRING, Some(WPARAM(0)), Some(LPARAM(t.as_ptr() as isize)));
    }
}

/// -1 clears the selection.
pub fn combo_set(h: HWND, i: i32) {
    unsafe {
        SendMessageW(h, CB_SETCURSEL, Some(WPARAM(i as isize as usize)), Some(LPARAM(0)));
    }
}

/// Widen the dropped-down list so long items (e.g. font names) aren't clipped.
pub fn combo_set_dropped_width(h: HWND, w: i32) {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{CB_SETDROPPEDWIDTH, SendMessageW};
        let _ = SendMessageW(h, CB_SETDROPPEDWIDTH, Some(WPARAM(w as usize)), None);
    }
}

/// Set the fixed item height for an owner-draw combo box. Required for
// CBS_OWNERDRAWFIXED combos — otherwise Windows uses a default (~16px)
// item height that clips tall fonts.
pub fn combo_set_item_height(h: HWND, height: i32) {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{CB_SETITEMHEIGHT, SendMessageW};
        let _ = SendMessageW(h, CB_SETITEMHEIGHT, Some(WPARAM(height as usize)), None);
    }
}

pub fn combo_sel(h: HWND) -> i32 {
    unsafe { SendMessageW(h, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0 as i32 }
}

pub fn set_text(h: HWND, s: &str) {
    let t = window::utf16z(s);
    unsafe {
        let _ = SetWindowTextW(h, PCWSTR(t.as_ptr()));
    }
}

pub fn get_text(h: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(h);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(h, &mut buf);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

pub fn set_readonly(h: HWND, v: bool) {
    unsafe {
        SendMessageW(h, EM_SETREADONLY, Some(WPARAM(v as usize)), Some(LPARAM(0)));
    }
}

/// Owner-drawn checkbox state, stored in GWLP_USERDATA (see `checkbox`).
pub fn checked(h: HWND) -> bool {
    unsafe { GetWindowLongPtrW(h, GWLP_USERDATA) != 0 }
}

pub fn set_checked(h: HWND, v: bool) {
    unsafe {
        SetWindowLongPtrW(h, GWLP_USERDATA, v as isize);
        let _ = InvalidateRect(Some(h), None, false);
    }
}

pub fn toggle_check(h: HWND) {
    set_checked(h, !checked(h));
}

pub fn enable(h: HWND, v: bool) {
    unsafe {
        let _ = EnableWindow(h, v);
    }
}

pub fn cue_banner(h: HWND, text: &str) {
    let t = window::utf16z(text);
    unsafe {
        SendMessageW(h, EM_SETCUEBANNER, Some(WPARAM(1)), Some(LPARAM(t.as_ptr() as isize)));
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ButtonKind {
    Accent,
    Normal,
    NavActive,
    NavIdle,
}

/// WM_DRAWITEM painter for `button` controls; nav kinds are start-aligned.
pub fn draw_button(dis: &DRAWITEMSTRUCT, text: &str, font: HFONT, kind: ButtonKind, rtl: bool, primary: bool, selected: bool) {
    let p = theme::current();

    let hovered = (dis.itemState.0 & ODS_HOTLIGHT.0) != 0;
    let pressed = (dis.itemState.0 & ODS_SELECTED.0) != 0;

    let is_active = selected || pressed;

    let (fill, border, fg) = if primary {
        if pressed {
            (mix(p.accent, p.bg, 0.15), None, p.bg)
        } else if hovered {
            (mix(p.accent, 0xFFFFFFFF, 0.10), None, p.bg)
        } else {
            (p.accent, None, p.bg)
        }
    } else if is_active {
        (p.accent, None, p.bg)
    } else if hovered {
        (p.bg_hover, Some(p.accent), p.text)
    } else {
        (p.panel, Some(theme::BORDER), p.text)
    };

    let fg = if (dis.itemState.0 & ODS_DISABLED.0) != 0 { p.text_dim } else { fg };

    unsafe {
        // Draw 1px border as a full rounded rect, then inset fill for a clean border.
        if let Some(border_color) = border {
            fill_round_rect(
                dis.hDC,
                dis.rcItem.left, dis.rcItem.top,
                dis.rcItem.right - dis.rcItem.left, dis.rcItem.bottom - dis.rcItem.top,
                6,
                border_color,
            );
        }
        // Inner fill (1px inset when there's a border, full when active/no border)
        let mut inner = dis.rcItem;
        if border.is_some() {
            inner.left += 1;
            inner.top += 1;
            inner.right -= 1;
            inner.bottom -= 1;
        }
        fill_round_rect(
            dis.hDC,
            inner.left, inner.top,
            inner.right - inner.left, inner.bottom - inner.top,
            6,
            fill,
        );

        SetBkMode(dis.hDC, TRANSPARENT);
        SetTextColor(dis.hDC, colorref(fg));
        let old = SelectObject(dis.hDC, font.into());
        let mut rc = dis.rcItem;
        if pressed {
            rc.top += 1;
            rc.bottom += 1;
        }
        let nav = matches!(kind, ButtonKind::NavActive | ButtonKind::NavIdle);
        let flags = if nav {
            rc.left += 10;
            rc.right -= 10;
            DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | if rtl { DT_RIGHT } else { DT_LEFT }
        } else {
            DT_VCENTER | DT_SINGLELINE | DT_CENTER
        };
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(dis.hDC, &mut wide, &mut rc, flags);
        SelectObject(dis.hDC, old);
    }
}

/// WM_DRAWITEM painter for owner-drawn combos (selected/edit field + dropdown list items).
/// Disabled selected text dims to `text_dim` instead of the theme's system grey.
pub fn draw_combo(dis: &DRAWITEMSTRUCT, font: HFONT, rtl: bool) {
    let p = theme::current();
    let disabled = (dis.itemState.0 & ODS_DISABLED.0) != 0;
    let selected = (dis.itemState.0 & ODS_SELECTED.0) != 0;
    let is_edit = (dis.itemState.0 & ODS_COMBOBOXEDIT.0) != 0;
    unsafe {
        let bg = if selected && !is_edit { p.accent_soft } else { p.bg_hover };
        // Inner rounded fill (1px inset)
        let mut inner = dis.rcItem;
        inner.left += 1;
        inner.top += 1;
        inner.right -= 1;
        inner.bottom -= 1;
        fill_round_rect(
            dis.hDC,
            inner.left, inner.top,
            inner.right - inner.left, inner.bottom - inner.top,
            6,
            bg,
        );

        if dis.itemID == u32::MAX {
            return; // empty combo, no selection to draw
        }
        let text = combo_item_text(dis.hwndItem, dis.itemID);
        SetBkMode(dis.hDC, TRANSPARENT);
        SetTextColor(dis.hDC, colorref(if disabled { p.text_dim } else { p.text }));
        let old = SelectObject(dis.hDC, font.into());
        let pad = (dis.rcItem.bottom - dis.rcItem.top) / 3;
        let mut rc = dis.rcItem;
        rc.left += pad + 1; // extra 1px for border
        rc.right -= 1;
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let flags = DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | if rtl { DT_RIGHT } else { DT_LEFT };
        DrawTextW(dis.hDC, &mut wide, &mut rc, flags);
        SelectObject(dis.hDC, old);
    }
}

/// WM_DRAWITEM painter for owner-drawn checkboxes: a flat rounded glyph (accent-filled with a
/// checkmark when checked, dim outline otherwise) that matches the dark theme, plus a label whose
/// color follows enabled/disabled so disabled text stays readable (`text_dim`) not system grey.
pub fn draw_checkbox(dis: &DRAWITEMSTRUCT, text: &str, font: HFONT, is_checked: bool, rtl: bool) {
    let p = theme::current();
    let disabled = (dis.itemState.0 & ODS_DISABLED.0) != 0;
    let rc = dis.rcItem;
    let row_h = rc.bottom - rc.top;
    let box_sz = (row_h * 13 / 20).max(12);
    let gap = (row_h / 3).max(4);
    let gy = rc.top + (row_h - box_sz) / 2;
    unsafe {
        let (gx, tl, tx_right) = if rtl {
            (rc.right - box_sz, rc.left, rc.right - box_sz - gap)
        } else {
            (rc.left, rc.left + box_sz + gap, rc.right)
        };

        // Rounded box: accent fill when checked (dim outline otherwise); everything dims when disabled.
        let border = if disabled { p.text_dim } else if is_checked { p.accent } else { p.text_dim };
        let fill = if is_checked && !disabled { p.accent } else { p.panel };
        let radius = (box_sz / 4).max(2);
        let pen = CreatePen(PS_SOLID, (box_sz / 14).max(1), colorref(border));
        let br = solid(fill);
        let op = SelectObject(dis.hDC, pen.into());
        let ob = SelectObject(dis.hDC, br.into());
        fill_round_rect(dis.hDC, gx, gy, box_sz, box_sz, radius, fill);
        SelectObject(dis.hDC, op);
        SelectObject(dis.hDC, ob);
        let _ = DeleteObject(pen.into());
        let _ = DeleteObject(br.into());

        if is_checked {
            let check = if disabled { p.text_dim } else { on_accent(p.accent) };
            let cpen = CreatePen(PS_SOLID, (box_sz / 8).max(2), colorref(check));
            let ocp = SelectObject(dis.hDC, cpen.into());
            let pts = [
                POINT { x: gx + box_sz * 22 / 100, y: gy + box_sz * 52 / 100 },
                POINT { x: gx + box_sz * 42 / 100, y: gy + box_sz * 70 / 100 },
                POINT { x: gx + box_sz * 75 / 100, y: gy + box_sz * 30 / 100 },
            ];
            let _ = Polyline(dis.hDC, &pts);
            SelectObject(dis.hDC, ocp);
            let _ = DeleteObject(cpen.into());
        }

        SetBkMode(dis.hDC, TRANSPARENT);
        SetTextColor(dis.hDC, colorref(if disabled { p.text_dim } else { p.text }));
        let old = SelectObject(dis.hDC, font.into());
        let mut tr = RECT { left: tl, top: rc.top, right: tx_right, bottom: rc.bottom };
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let flags = DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | if rtl { DT_RIGHT } else { DT_LEFT };
        DrawTextW(dis.hDC, &mut wide, &mut tr, flags);
        SelectObject(dis.hDC, old);
    }
}

/// Installed font family names, sorted, no vertical (@-prefixed) faces.
pub fn font_families() -> Vec<String> {
    unsafe extern "system" fn cb(lf: *const LOGFONTW, _tm: *const TEXTMETRICW, _ty: u32, lp: LPARAM) -> i32 {
        let list = unsafe { &mut *(lp.0 as *mut Vec<String>) };
        let face = unsafe { &(*lf).lfFaceName };
        let len = face.iter().position(|&c| c == 0).unwrap_or(face.len());
        let name = String::from_utf16_lossy(&face[..len]);
        if !name.starts_with('@') && !list.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            list.push(name);
        }
        1
    }
    let mut list: Vec<String> = Vec::new();
    unsafe {
        let hdc = GetDC(None);
        let lf = LOGFONTW {
            lfCharSet: DEFAULT_CHARSET,
            ..Default::default()
        };
        EnumFontFamiliesExW(hdc, &lf, Some(cb), LPARAM(&mut list as *mut _ as isize), 0);
        ReleaseDC(None, hdc);
    }
    list.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    list
}

/// Returns text of combo item at idx.
pub fn combo_item_text(h: HWND, idx: u32) -> String {
    unsafe {
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

/// Owner-drawn push button; paint it from WM_DRAWITEM via `draw_button`.
pub fn button(parent: HWND, text: &str, x: i32, y: i32, w: i32, h: i32, id: i32) -> HWND {
    create(parent, w!("BUTTON"), text, WS_TABSTOP.0 | BS_OWNERDRAW as u32, x, y, w, h, id)
}

/// Fill a WM_CTLCOLOR* HDC and return the brush to hand back as the LRESULT.
pub fn ctl_colors(hdc: HDC, text: u32, bg: u32, brush: HBRUSH) -> HBRUSH {
    unsafe {
        SetTextColor(hdc, colorref(text));
        windows::Win32::Graphics::Gdi::SetBkColor(hdc, colorref(bg));
    }
    brush
}
