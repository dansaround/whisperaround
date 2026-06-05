//! Floating-window layout helpers shared by the setup code (`lib.rs`) and the
//! IPC commands (`commands.rs`). The `main` window is a single always-on-top
//! surface that resizes to fit whatever the pill is showing — an empty pill at
//! rest, the icon row / tooltip on hover, the model menu, the recording state,
//! or the expanded panel. Resizes keep the window's current anchor so the pill
//! stays where the user dragged it instead of snapping back to the top.

use tauri::{LogicalSize, PhysicalPosition, WebviewWindow};

/// Expanded panel size (logical px).
pub const PANEL_SIZE: (f64, f64) = (580.0, 240.0);
/// Gap between the window and the top edge of the screen (logical px).
const TOP_MARGIN: f64 = 12.0;

/// Which point of the window stays fixed across a resize.
pub enum Anchor {
    /// Keep the top edge and horizontal center (compact pill: grows downward
    /// and symmetrically, so it never jumps sideways or upward).
    TopCenter,
    /// Keep the geometric center (expanding/collapsing the panel).
    Center,
}

/// Logical size for each compact UI mode requested by the frontend.
pub fn size_for_mode(mode: &str) -> (f64, f64) {
    match mode {
        "icons" => (160.0, 54.0), // pill with the 3 icons, no tooltip
        "tip" => (230.0, 112.0),  // icons + hover tooltip below
        "menu" => (230.0, 208.0), // icons + model dropdown
        "rec" => (130.0, 54.0),   // recording / processing
        _ => (96.0, 40.0),        // idle: empty pill
    }
}

/// The resting (empty) pill size.
pub fn pill_size() -> (f64, f64) {
    size_for_mode("idle")
}

/// Resize and re-pin to the top-center of the active monitor. Used on launch
/// and when revealing the pill from the tray (a deliberate "reset to top").
pub fn layout_main(window: &WebviewWindow, size: (f64, f64)) {
    let _ = window.set_size(LogicalSize::new(size.0, size.1));
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let screen = monitor.size();
    let origin = monitor.position();
    let win_w = size.0 * scale;
    let x = origin.x as f64 + (screen.width as f64 - win_w) / 2.0;
    let y = origin.y as f64 + TOP_MARGIN * scale;
    let _ = window.set_position(PhysicalPosition::new(x.round(), y.round()));
}

/// Resize while keeping the given anchor point fixed, clamped to the monitor.
pub fn resize_anchored(window: &WebviewWindow, size: (f64, f64), anchor: Anchor) {
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
    let mut ny = match anchor {
        Anchor::TopCenter => pos.y as f64,
        Anchor::Center => pos.y as f64 + cur.height as f64 / 2.0 - new_h / 2.0,
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

    let _ = window.set_size(LogicalSize::new(size.0, size.1));
    let _ = window.set_position(PhysicalPosition::new(nx.round(), ny.round()));
}
