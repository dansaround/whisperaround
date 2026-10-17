//! Windows-only integration: the app the user is dictating into (history's
//! "Apps used") and whether the settings window gets the Mica backdrop.

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

/// Mica (the `mica` entry in `windowEffects`, tauri.conf.json) needs Windows
/// 11; on Windows 10 the settings window stays opaque.
pub fn mica_supported() -> bool {
    windows_version::OsVersion::current().build >= 22000
}

/// Name of the app the user is working in, ignoring WhisperAround itself
/// (e.g. after a click on the pill): the executable's description ("Google
/// Chrome", "Microsoft Word"), else its file name.
pub fn frontmost_app_name() -> Option<String> {
    // SAFETY: plain Win32 getters; every handle is checked and closed.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == std::process::id() {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let stem = std::path::Path::new(&path).file_stem()?.to_string_lossy().into_owned();
        // Store apps are all hosted by ApplicationFrameHost; their window
        // title is the app's name.
        if stem.eq_ignore_ascii_case("ApplicationFrameHost") {
            let mut title = [0u16; 256];
            let n = GetWindowTextW(hwnd, &mut title);
            return (n > 0).then(|| String::from_utf16_lossy(&title[..n as usize]));
        }
        Some(file_description(&path).unwrap_or(stem))
    }
}

/// `FileDescription` from the executable's version resource.
fn file_description(path: &str) -> Option<String> {
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let file = PCWSTR(wide.as_ptr());
    // SAFETY: the buffer is sized by GetFileVersionInfoSizeW, and the
    // pointers VerQueryValueW returns point into it while it lives.
    unsafe {
        let size = GetFileVersionInfoSizeW(file, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(file, None, size, data.as_mut_ptr().cast()).ok()?;

        // First language / code page pair.
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        let query: Vec<u16> = "\\VarFileInfo\\Translation".encode_utf16().chain(Some(0)).collect();
        if !VerQueryValueW(data.as_ptr().cast(), PCWSTR(query.as_ptr()), &mut ptr, &mut len).as_bool() || len < 4 {
            return None;
        }
        let pair = ptr as *const u16;
        let (lang, code_page) = (*pair, *pair.add(1));

        let key = format!("\\StringFileInfo\\{lang:04x}{code_page:04x}\\FileDescription");
        let query: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
        if !VerQueryValueW(data.as_ptr().cast(), PCWSTR(query.as_ptr()), &mut ptr, &mut len).as_bool() || len == 0 {
            return None;
        }
        let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let name = String::from_utf16_lossy(text).trim_end_matches('\0').trim().to_string();
        (!name.is_empty()).then_some(name)
    }
}
