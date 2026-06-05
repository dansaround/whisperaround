// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Under WSLg the WebKitGTK webview renders through software EGL (ZINK), which
    // leaves the window blank/glitchy unless GPU compositing is disabled. This is
    // a no-op on native Windows (WebView2) and is only applied when WSL is
    // detected, so native Linux GPUs keep hardware compositing.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WSL_DISTRO_NAME").is_some() || std::env::var_os("WSL_INTEROP").is_some() {
        // Force the X11 backend (WSLg's Wayland path leaves the webview black),
        // software GL (no real GPU under WSLg), and disable webview compositing.
        std::env::set_var("GDK_BACKEND", "x11");
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    whisperaround_lib::run();
}
