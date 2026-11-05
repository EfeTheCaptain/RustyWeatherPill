//! Taskbar lookup/geometry, fullscreen detection, DPI, z-order — port of Native/Interop.cs.

use super::displays;
use windows::core::w;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{MonitorFromRect, MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetWindowPos, GA_ROOT, HWND_TOPMOST,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
};

pub fn taskbar() -> Option<HWND> {
    unsafe { FindWindowW(w!("Shell_TrayWnd"), None).ok().filter(|h| !h.is_invalid()) }
}

/// Taskbar window for a monitor device: primary's Shell_TrayWnd, or the matching
/// Shell_SecondaryTrayWnd; falls back to primary.
pub fn taskbar_for_device(device: Option<&str>) -> Option<HWND> {
    let primary = taskbar();
    let Some(d) = device.filter(|d| !d.is_empty()) else { return primary };
    if let Some(p) = displays::primary() {
        if p.device.eq_ignore_ascii_case(d) {
            return primary;
        }
    }
    let mut h = HWND::default();
    loop {
        h = match unsafe { FindWindowExW(None, Some(h), w!("Shell_SecondaryTrayWnd"), None) } {
            Ok(w) if !w.is_invalid() => w,
            _ => return primary,
        };
        if let Some(m) = displays::from_window(h) {
            if m.device.eq_ignore_ascii_case(d) {
                return Some(h);
            }
        }
    }
}

pub fn window_rect(h: HWND) -> Option<RECT> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(h, &mut r) }.is_ok().then_some(r)
}

/// Screen-x of the taskbar's tray (clock cluster) left edge, or 0 if not found.
pub fn tray_notify_left(taskbar: HWND) -> i32 {
    match unsafe { FindWindowExW(Some(taskbar), None, w!("TrayNotifyWnd"), None) } {
        Ok(h) if !h.is_invalid() => window_rect(h).map(|r| r.left).unwrap_or(0),
        _ => 0,
    }
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Pixels by which a fullscreen window may miss the monitor edge and still count
/// (GPU scaling / DWM rounding). Kept small so a merely maximized window doesn't qualify.
const FULLSCREEN_TOLERANCE: i32 = 2;

fn covers(r: &RECT, bounds: &RECT, tol: i32) -> bool {
    r.left <= bounds.left + tol
        && r.top <= bounds.top + tol
        && r.right >= bounds.right - tol
        && r.bottom >= bounds.bottom - tol
}

/// True for the shell states that mean "a fullscreen app/game/video is running".
fn quns_is_fullscreen(state: i32) -> bool {
    state == QUNS_BUSY.0 || state == QUNS_RUNNING_D3D_FULL_SCREEN.0 || state == QUNS_PRESENTATION_MODE.0
}

/// True if the foreground window is a real app covering the whole given monitor.
pub fn is_fullscreen_app_on(bounds: &RECT) -> bool {
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_invalid() {
        return false;
    }
    // The foreground window can be a child/owned window; the root carries the real frame.
    let root = unsafe { GetAncestor(fg, GA_ROOT) };
    let root = if root.is_invalid() { fg } else { root };
    if unsafe { IsIconic(root) }.as_bool() || !unsafe { IsWindowVisible(root) }.as_bool() {
        return false;
    }
    match class_name(root).as_str() {
        "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "WorkerW" | "Progman" => return false,
        _ => {}
    }

    // Only react to a fullscreen app on the monitor we were asked about; a game on
    // another display must not hide this monitor's pill.
    let target = unsafe { MonitorFromRect(bounds, MONITOR_DEFAULTTONEAREST) };
    let window_mon = unsafe { MonitorFromWindow(root, MONITOR_DEFAULTTONEAREST) };
    if target.is_invalid() || window_mon.is_invalid() || target != window_mon {
        return false;
    }

    if let Some(r) = window_rect(root) {
        if covers(&r, bounds, FULLSCREEN_TOLERANCE) {
            return true;
        }
    }

    // Exclusive/D3D fullscreen renders through a surface the HWND rectangle does not
    // reflect, so the rect test misses it. Ask the shell, which knows about that state.
    unsafe { SHQueryUserNotificationState() }
        .map(|s| quns_is_fullscreen(s.0))
        .unwrap_or(false)
}

/// Re-assert top-most z-order without moving/activating.
pub fn raise_topmost(h: HWND) {
    unsafe {
        let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

pub fn scale_of(h: HWND) -> f32 {
    if h.is_invalid() {
        return 1.0;
    }
    let dpi = unsafe { GetDpiForWindow(h) };
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / 96.0
    }
}

pub fn thread_of(h: HWND) -> u32 {
    unsafe { GetWindowThreadProcessId(h, None) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(l: i32, t: i32, r: i32, b: i32) -> RECT {
        RECT { left: l, top: t, right: r, bottom: b }
    }

    #[test]
    fn coverage_accepts_fullscreen_and_rejects_work_area() {
        let bounds = rect(0, 0, 1920, 1080);
        assert!(covers(&rect(0, 0, 1920, 1080), &bounds, FULLSCREEN_TOLERANCE));
        assert!(covers(&rect(-1, -1, 1921, 1081), &bounds, FULLSCREEN_TOLERANCE));
        // Maximized normal window stops at the work area (taskbar visible).
        assert!(!covers(&rect(0, 0, 1920, 1040), &bounds, FULLSCREEN_TOLERANCE));
        // Small window never qualifies.
        assert!(!covers(&rect(100, 100, 800, 600), &bounds, FULLSCREEN_TOLERANCE));
    }

    #[test]
    fn quns_busy_d3d_and_presentation_are_fullscreen() {
        assert!(quns_is_fullscreen(QUNS_BUSY.0));
        assert!(quns_is_fullscreen(QUNS_RUNNING_D3D_FULL_SCREEN.0));
        assert!(quns_is_fullscreen(QUNS_PRESENTATION_MODE.0));
        // Accepting notifications / quiet time / away / store-app are not treated as fullscreen.
        assert!(!quns_is_fullscreen(1));
        assert!(!quns_is_fullscreen(5));
        assert!(!quns_is_fullscreen(6));
        assert!(!quns_is_fullscreen(7));
    }
}
