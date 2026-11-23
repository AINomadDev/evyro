//! Menu bar (macOS) / system tray (Windows, Linux) icon.
//!
//! Closing the window while agents are still running hides it instead of
//! stopping them; the tray icon brings it back or quits for real. With nothing
//! running, closing the window quits as before.

use crate::{chat, threads};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime, WindowEvent};

/// Agent threads and chats still running in this window.
pub fn running_work(terms: &threads::Terms, chats: &chat::Chats) -> usize {
    terms.list().iter().filter(|t| t.running).count() + chats.host.list().len()
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Linux needs libayatana-appindicator3 (or libappindicator3) for a tray
/// icon, and the library loader panics when neither is installed, so look
/// first. The .deb depends on it; this covers other installs.
#[cfg(target_os = "linux")]
fn tray_library_present() -> bool {
    const LIBS: [&str; 4] = [
        "libayatana-appindicator3.so.1",
        "libappindicator3.so.1",
        "libayatana-appindicator3.so",
        "libappindicator3.so",
    ];
    for ldconfig in ["ldconfig", "/sbin/ldconfig", "/usr/sbin/ldconfig"] {
        if let Ok(out) = std::process::Command::new(ldconfig).arg("-p").output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                return LIBS.iter().any(|l| text.contains(l));
            }
        }
    }
    let dirs = [
        "/usr/lib",
        "/usr/lib64",
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
        "/usr/local/lib",
    ];
    dirs.iter().any(|d| {
        LIBS.iter()
            .any(|l| std::path::Path::new(d).join(l).exists())
    })
}

#[cfg(not(target_os = "linux"))]
fn tray_library_present() -> bool {
    true
}

pub fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if !tray_library_present() {
        eprintln!("evyro: no tray icon (install libayatana-appindicator3-1 for one)");
        return Ok(());
    }
    let show = MenuItem::with_id(app, "show", "Show Evyro", true, None::<&str>)?;
    let quit = MenuItem::with_id(
        app,
        "quit",
        "Quit Evyro (stops running agents)",
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&show, &PredefinedMenuItem::separator(app)?, &quit])?;
    let mut b = TrayIconBuilder::with_id("evyro")
        .tooltip("Evyro")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app)?;
    Ok(())
}

/// Hide instead of close while work is running, and say so once.
pub fn on_window_event<R: Runtime>(
    window: &tauri::Window<R>,
    event: &WindowEvent,
    terms: &threads::Terms,
    chats: &chat::Chats,
) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        let n = running_work(terms, chats);
        // Without a tray icon there would be no way back to a hidden window.
        if n == 0 || window.app_handle().tray_by_id("evyro").is_none() {
            return;
        }
        api.prevent_close();
        let _ = window.hide();
        use tauri_plugin_notification::NotificationExt;
        let _ = window
            .app_handle()
            .notification()
            .builder()
            .title("Evyro is still running")
            .body(format!(
                "{n} agent{} still working. Open Evyro from the {} to see them, or quit there to stop them.",
                if n == 1 { " is" } else { "s are" },
                if cfg!(target_os = "macos") { "menu bar" } else { "tray" }
            ))
            .show();
    }
}
