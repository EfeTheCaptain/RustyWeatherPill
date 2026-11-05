//! Tray icon (Shell_NotifyIconW) with balloon fallback, port of the NotifyIcon usage in AppHost.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIIF_NOSOUND, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, HICON, WM_APP,
};

pub const WM_TRAY: u32 = WM_APP + 1;
const TRAY_ID: u32 = 1;

pub struct TrayIcon {
    hwnd: HWND,
    icon: HICON,
    added: bool,
}

fn copy_to(dst: &mut [u16], s: &str) {
    let mut n = 0;
    for u in s.encode_utf16() {
        if n >= dst.len() - 1 {
            break;
        }
        dst[n] = u;
        n += 1;
    }
    dst[n] = 0;
}

impl TrayIcon {
    pub fn new(hwnd: HWND) -> Self {
        // No app.ico here: the tray only ever shows a weather condition icon,
        // set on the first weather result via sync_tray_icon.
        let mut tray = Self { hwnd, icon: HICON::default(), added: false };
        tray.add();
        tray
    }

    fn base(&self) -> NOTIFYICONDATAW {
        // No uVersion set: classic callback semantics — lParam of WM_TRAY carries the
        // raw mouse message (WM_MOUSEMOVE / WM_LBUTTONUP / WM_RBUTTONUP / ...).
        NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ID,
            ..Default::default()
        }
    }

    fn add(&mut self) {
        let mut d = self.base();
        d.uFlags = NIF_MESSAGE | NIF_TIP;
        d.uCallbackMessage = WM_TRAY;
        // No app.ico: only advertise an icon once a weather condition icon exists.
        if !self.icon.is_invalid() {
            d.uFlags |= NIF_ICON;
            d.hIcon = self.icon;
        }
        self.added = unsafe { Shell_NotifyIconW(NIM_ADD, &d) }.as_bool();
    }

    /// Re-adds the icon after Explorer restarts (TaskbarCreated broadcast).
    pub fn readd(&mut self) {
        self.added = false;
        self.add();
    }

    /// Swap the tray icon (weather condition changed). `icon` ownership is
    /// transferred; the previous icon is destroyed. No-op for invalid handles.
    pub fn set_icon(&mut self, icon: HICON) {
        if icon.is_invalid() {
            return;
        }
        let old = std::mem::replace(&mut self.icon, icon);
        if self.added {
            let mut d = self.base();
            d.uFlags = NIF_ICON;
            d.hIcon = self.icon;
            unsafe { let _ = Shell_NotifyIconW(NIM_MODIFY, &d); }
        }
        if !old.is_invalid() {
            unsafe { let _ = DestroyIcon(old); }
        }
    }

    /// Tooltip is capped at 127 chars by the shell.
    pub fn set_tooltip(&mut self, text: &str) {
        let mut d = self.base();
        d.uFlags = NIF_TIP;
        copy_to(&mut d.szTip, text);
        unsafe { let _ = Shell_NotifyIconW(NIM_MODIFY, &d); }
    }

    pub fn balloon(&mut self, title: &str, body: &str, silent: bool) {
        let mut d = self.base();
        d.uFlags = NIF_INFO;
        d.dwInfoFlags = if silent { NIIF_INFO | NIIF_NOSOUND } else { NIIF_INFO };
        copy_to(&mut d.szInfoTitle, title);
        copy_to(&mut d.szInfo, body);
        unsafe { let _ = Shell_NotifyIconW(NIM_MODIFY, &d); }
    }

    pub fn remove(&mut self) {
        if self.added {
            let d = self.base();
            unsafe { let _ = Shell_NotifyIconW(NIM_DELETE, &d); }
            self.added = false;
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        self.remove();
        if !self.icon.is_invalid() {
            unsafe { let _ = DestroyIcon(self.icon); }
        }
    }
}
