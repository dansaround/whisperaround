//! Floating-window layout helpers shared by the setup code (`lib.rs`) and the
//! IPC commands (`commands.rs`). The `main` window is a single always-on-top
//! surface that resizes to fit whatever the pill is showing — an empty pill at
//! rest, the icon row / tooltip on hover, the model menu, the recording state,
//! or the expanded panel. Resizes keep the top-center (or, with the pill at the
//! bottom of the screen, the bottom-center) fixed so the pill stays where the
//! user dragged it instead of snapping back.

use tauri::{LogicalSize, PhysicalPosition, WebviewWindow};

/// Resting (empty) pill window size (logical px). Must match `IDLE` in
/// `src/lib/Pill.svelte`, which drives every other size.
pub const PILL_SIZE: (f64, f64) = (52.0, 34.0);
/// Gap between the window and the top/bottom edge of the usable screen area
/// (logical px).
const EDGE_MARGIN: f64 = 12.0;

/// Resize and re-pin to the top- or bottom-center of the active monitor's
/// usable area (below the menu bar / above the Dock). Used on launch, when the
/// position setting changes and when revealing the pill from the tray.
pub fn layout_main(window: &WebviewWindow, size: (f64, f64), bottom: bool) {
    let _ = window.set_size(LogicalSize::new(size.0, size.1));
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let (win_w, win_h) = (size.0 * scale, size.1 * scale);
    let x = area.position.x as f64 + (area.size.width as f64 - win_w) / 2.0;
    let y = if bottom {
        area.position.y as f64 + area.size.height as f64 - win_h - EDGE_MARGIN * scale
    } else {
        area.position.y as f64 + EDGE_MARGIN * scale
    };
    let _ = window.set_position(PhysicalPosition::new(x.round(), y.round()));
}

/// Resize keeping the horizontal center and the top edge (or the bottom edge
/// when `bottom`) fixed, so the pill grows away from the screen edge it sits
/// on. Clamped to the monitor.
pub fn resize_anchored(window: &WebviewWindow, size: (f64, f64), bottom: bool) {
    let (pos, cur) = match (window.outer_position(), window.outer_size()) {
        (Ok(p), Ok(c)) => (p, c),
        _ => {
            let _ = window.set_size(LogicalSize::new(size.0, size.1));
            return;
        }
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let new_w = size.0 * scale;
    let new_h = size.1 * scale;

    let center_x = pos.x as f64 + cur.width as f64 / 2.0;
    let mut nx = center_x - new_w / 2.0;
    let mut ny = if bottom {
        pos.y as f64 + cur.height as f64 - new_h
    } else {
        pos.y as f64
    };

    // Keep the whole window on the active monitor.
    if let Ok(Some(m)) = window.current_monitor() {
        let ms = m.size();
        let mo = m.position();
        let max_x = mo.x as f64 + ms.width as f64 - new_w;
        let max_y = mo.y as f64 + ms.height as f64 - new_h;
        nx = nx.clamp(mo.x as f64, max_x.max(mo.x as f64));
        ny = ny.clamp(mo.y as f64, max_y.max(mo.y as f64));
    }

    set_bounds(window, nx.round() as i32, ny.round() as i32, new_w.round() as i32, new_h.round() as i32);
}

/// Move + resize in one native call where possible. Two separate calls (size,
/// then position) briefly show the content off-center — a visible jump.
#[cfg(windows)]
fn set_bounds(window: &WebviewWindow, x: i32, y: i32, w: i32, h: i32) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER,
    };
    if let Ok(hwnd) = window.hwnd() {
        let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
        // SAFETY: `hwnd` is the live native handle of this Tauri window.
        if unsafe { SetWindowPos(HWND(hwnd.0), None, x, y, w, h, flags) }.is_ok() {
            return;
        }
    }
    let _ = window.set_size(tauri::PhysicalSize::new(w as u32, h as u32));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

#[cfg(not(windows))]
fn set_bounds(window: &WebviewWindow, x: i32, y: i32, w: i32, h: i32) {
    let _ = window.set_size(tauri::PhysicalSize::new(w as u32, h as u32));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// Whether the user put the pill at the bottom of the screen.
pub fn pill_at_bottom(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    app.state::<crate::state::AppState>()
        .settings
        .lock()
        .unwrap()
        .pill_position
        == "bottom"
}

/// Give the titled windows their title in the current UI language (on launch
/// and whenever the language changes).
pub fn localize_titles(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(welcome) = app.get_webview_window("welcome") {
        let _ = welcome.set_title(&crate::i18n::t("window.welcomeTitle"));
    }
}
