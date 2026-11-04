#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;

fn main() {
    // Single instance: mutex held (leaked) for the process lifetime.
    let mutex = unsafe { CreateMutexW(None, true, w!("RustyWeatherPill.SingleInstance")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return;
    }
    let _ = std::mem::ManuallyDrop::new(mutex); // never released; the OS reclaims at exit

    rusty_weather_pill::app::App::run();
}
