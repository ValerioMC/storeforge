use objc2::{AllocAnyThread, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::NSData;
use tauri::{AppHandle, Manager, RunEvent, Theme, Window, WindowEvent};

const LIGHT_ICON: &[u8] = include_bytes!("../icons/icon.png");
const DARK_ICON: &[u8] = include_bytes!("../icons/dock-dark.png");

pub fn on_app_event(app: &AppHandle, event: RunEvent) {
    if let RunEvent::Ready = event
        && let Some(window) = app.get_webview_window("main")
    {
        match window.theme() {
            Ok(theme) => update(theme),
            Err(error) => tracing::warn!(%error, "Could not read the macOS appearance"),
        }
    }
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() == "main"
        && let WindowEvent::ThemeChanged(theme) = event
    {
        let theme = *theme;
        if let Err(error) = window.run_on_main_thread(move || update(theme)) {
            tracing::warn!(%error, "Could not update the macOS Dock icon");
        }
    }
}

fn update(theme: Theme) {
    let Some(main_thread) = MainThreadMarker::new() else {
        tracing::warn!("The macOS Dock icon update requires the main thread");
        return;
    };
    let bytes = if theme == Theme::Dark {
        DARK_ICON
    } else {
        LIGHT_ICON
    };
    let data = NSData::with_bytes(bytes);
    let Some(icon) = NSImage::initWithData(NSImage::alloc(), &data) else {
        tracing::warn!(?theme, "Could not decode the macOS Dock icon");
        return;
    };
    let app = NSApplication::sharedApplication(main_thread);
    unsafe { app.setApplicationIconImage(Some(&icon)) };
}
