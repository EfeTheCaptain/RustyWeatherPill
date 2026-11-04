//! Minimal HTTPS GET over WinHTTP (OS schannel TLS, zero extra deps).
//! Port of TaskbarPray services/http.rs, trimmed to what the weather service needs.

use windows::core::{w, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

struct Handle(*mut core::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Url {
    host: Vec<u16>,
    path: Vec<u16>,
    port: u16,
    secure: bool,
}

fn crack(url: &str) -> Option<Url> {
    let (rest, secure) = url
        .strip_prefix("https://")
        .map(|r| (r, true))
        .or_else(|| url.strip_prefix("http://").map(|r| (r, false)))?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match host.rfind(':') {
        Some(i) => (&host[..i], host[i + 1..].parse().ok()?),
        None => (host, if secure { 443 } else { 80 }),
    };
    Some(Url { host: wide(host), path: wide(path), port, secure })
}

/// GET returning the body as UTF-8 (8 s timeouts like the C# HttpClient), None on failure.
pub fn get_string(url: &str) -> Option<String> {
    let Some(u) = crack(url) else { return None };
    unsafe {
        let session = Handle(WinHttpOpen(
            w!("RustyWeatherPill"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ));
        if session.0.is_null() {
            return None;
        }
        let _ = WinHttpSetTimeouts(session.0, 8_000, 8_000, 8_000, 8_000);

        let conn = Handle(WinHttpConnect(session.0, PCWSTR(u.host.as_ptr()), u.port, 0));
        if conn.0.is_null() {
            return None;
        }
        let req = Handle(WinHttpOpenRequest(
            conn.0,
            w!("GET"),
            PCWSTR(u.path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null_mut(),
            if u.secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) },
        ));
        if req.0.is_null() {
            return None;
        }
        if WinHttpSendRequest(req.0, None, None, 0, 0, 0).is_err()
            || WinHttpReceiveResponse(req.0, std::ptr::null_mut()).is_err()
        {
            return None;
        }

        let mut status = 0u32;
        let mut len = 4u32;
        if WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut core::ffi::c_void),
            &mut len,
            std::ptr::null_mut(),
        )
        .is_err()
            || !(200..300).contains(&status)
        {
            return None;
        }

        let mut body = Vec::new();
        let mut buf = [0u8; 16384];
        loop {
            let mut read = 0u32;
            if WinHttpReadData(req.0, buf.as_mut_ptr() as *mut core::ffi::c_void, buf.len() as u32, &mut read)
                .is_err()
            {
                return None;
            }
            if read == 0 {
                return Some(String::from_utf8_lossy(&body).into_owned());
            }
            body.extend_from_slice(&buf[..read as usize]);
            if body.len() > 8 * 1024 * 1024 {
                return None; // sanity cap; forecast/geocode responses are tiny
            }
        }
    }
}
