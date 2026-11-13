//! Taskbar lookup/geometry, fullscreen detection, DPI, z-order — port of Native/Interop.cs.

use super::displays;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, GetAncestor, GetClassNameW, GetWindowRect,
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

/// State carried through the `EnumWindows` callback while scanning for a fullscreen window.
struct FullscreenScan {
    target: RECT,
    found: bool,
}

/// Callback for `EnumWindows`: stops at the first window that fully covers `target`.
unsafe extern "system" fn fullscreen_enum_proc(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
    let scan = &mut *(lparam.0 as *mut FullscreenScan);
    if scan.found {
        return windows::Win32::Foundation::FALSE;
    }

    // The real frame is the root ancestor; owned overlays ride their owner and are skipped.
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    let root = if root.is_invalid() { hwnd } else { root };

    if unsafe { IsIconic(root) }.as_bool() {
        return windows::Win32::Foundation::TRUE;
    }
    if !unsafe { IsWindowVisible(root) }.as_bool() {
        return windows::Win32::Foundation::TRUE;
    }

    // Never treat shell windows or our own windows as the occupying fullscreen app.
    let class = class_name(root);
    if matches!(
        class.as_str(),
        "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "WorkerW"
            | "Progman"
            | "WeatherPillApp"
            | "WeatherPillWidget"
            | "WeatherPillForecast"
            | "WeatherPillSettings"
    ) {
        return windows::Win32::Foundation::TRUE;
    }

    // Suspended UWP windows are "visible" and full-size but do not occupy the screen.
    let mut cloaked: u32 = 0;
    if unsafe {
        DwmGetWindowAttribute(
            root,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok() && cloaked != 0
    } {
        return windows::Win32::Foundation::TRUE;
    }

    if let Some(r) = window_rect(root) {
        if covers(&r, &scan.target, FULLSCREEN_TOLERANCE) {
            scan.found = true;
            return windows::Win32::Foundation::FALSE;
        }
    }
    windows::Win32::Foundation::TRUE
}

/// True if any top-level window fully covers `bounds`.
///
/// Focus-independent: the foreground window plays no role, so clicking the taskbar on one
/// monitor cannot make the pill on another monitor reappear over a fullscreen game.
/// Recomputed fresh from OS state on every call, never cached.
pub fn is_fullscreen_app_on(bounds: &RECT) -> bool {
    let mut scan = FullscreenScan {
        target: *bounds,
        found: false,
    };
    unsafe {
        let _ = EnumWindows(
            Some(fullscreen_enum_proc),
            LPARAM(&mut scan as *mut FullscreenScan as isize),
        );
    }
    scan.found
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
}
