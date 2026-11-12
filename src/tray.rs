//! System tray icon (Windows only).
//!
//! When "Hide to the system tray when closing" is on, closing the window only hides it,
//! so fonts stay active and FontCraft keeps watching folders. The tray icon brings the
//! window back; its menu can switch all fonts off or quit FontCraft.
//!
//! While the window is hidden the app's own drawing loop is paused, so the tray talks
//! to the window directly through Windows.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_HIDE, SW_RESTORE, SW_SHOW};

pub struct Tray {
    _icon: TrayIcon,
    /// set when the person chose "Quit" in the tray menu
    pub quit: Arc<AtomicBool>,
    /// set when the person chose "Switch all fonts off"
    pub all_off: Arc<AtomicBool>,
}

pub fn show(hwnd: isize) {
    if hwnd != 0 {
        unsafe {
            ShowWindow(hwnd as _, SW_SHOW);
            ShowWindow(hwnd as _, SW_RESTORE);
            SetForegroundWindow(hwnd as _);
        }
    }
}

pub fn hide(hwnd: isize) {
    if hwnd != 0 {
        unsafe {
            ShowWindow(hwnd as _, SW_HIDE);
        }
    }
}

pub fn create(hwnd: Arc<AtomicIsize>, ctx: egui::Context) -> Result<Tray, String> {
    let icon = crate::icon_sized(32);
    let icon = Icon::from_rgba(icon.rgba, icon.width, icon.height).map_err(|e| e.to_string())?;
    let open = MenuItem::new("Show FontCraft", true, None);
    let off = MenuItem::new("Switch all fonts off", true, None);
    let quit = MenuItem::new("Quit FontCraft", true, None);
    let menu = Menu::new();
    menu.append_items(&[&open, &off, &PredefinedMenuItem::separator(), &quit]).map_err(|e| e.to_string())?;
    let tray = TrayIconBuilder::new()
        .with_tooltip("FontCraft")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|e| e.to_string())?;

    let quit_flag = Arc::new(AtomicBool::new(false));
    let off_flag = Arc::new(AtomicBool::new(false));
    {
        let hwnd = hwnd.clone();
        let ctx = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            let open = matches!(
                e,
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                    | TrayIconEvent::DoubleClick { .. }
            );
            if open {
                show(hwnd.load(Ordering::Relaxed));
                ctx.request_repaint();
            }
        }));
    }
    {
        let (open_id, off_id, quit_id) = (open.id().clone(), off.id().clone(), quit.id().clone());
        let (q, o) = (quit_flag.clone(), off_flag.clone());
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if e.id == quit_id {
                q.store(true, Ordering::Relaxed);
            } else if e.id == off_id {
                o.store(true, Ordering::Relaxed);
            } else if e.id != open_id {
                return;
            }
            // the window has to be visible for FontCraft to act on the choice
            show(hwnd.load(Ordering::Relaxed));
            ctx.request_repaint();
        }));
    }
    Ok(Tray { _icon: tray, quit: quit_flag, all_off: off_flag })
}
