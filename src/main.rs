#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod activation;
mod app;
mod config;
mod extras;
mod fontload;
mod fontmatch;
mod google;
mod library;
mod netfs;
mod online;
mod pdfprint;
mod remove;
mod sync;
mod autoact;
mod uifont;
mod updates;
mod watcher;
#[cfg(windows)]
mod tray;

use std::path::PathBuf;

/// The app icon (the platypus), as a 1024×1024 picture.
const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

/// The app icon at `size`×`size` pixels, for the window, the taskbar and the tray.
pub(crate) fn icon_sized(size: u32) -> egui::IconData {
    match image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png) {
        Ok(img) => {
            let img = img.resize_exact(size, size, image::imageops::FilterType::Lanczos3).to_rgba8();
            egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
        }
        Err(_) => fallback_icon(),
    }
}

pub(crate) fn icon() -> egui::IconData {
    icon_sized(256)
}

fn fallback_icon() -> egui::IconData {
    let n = 64usize;
    let mut rgba = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32, y as f32);
            // rounded square
            let r = 14.0;
            let cx = fx.clamp(r, n as f32 - 1.0 - r);
            let cy = fy.clamp(r, n as f32 - 1.0 - r);
            let inside = (fx - cx).powi(2) + (fy - cy).powi(2) <= r * r;
            let i = (y * n + x) * 4;
            if inside {
                // the letter F
                let letter = (18..26).contains(&x) && (14..50).contains(&y)
                    || (18..46).contains(&x) && (14..22).contains(&y)
                    || (18..40).contains(&x) && (29..36).contains(&y);
                let c = if letter { [255, 255, 255] } else { [0x6d, 0x5d, 0xfc] };
                rgba[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
    }
    egui::IconData { rgba, width: n as u32, height: n as u32 }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut shot = None;
    let mut minimized = false;
    // a picture given on the command line (\"Open with FontCraft\") goes to the Font finder
    let mut picture = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--screenshot" => {
                if let Some(p) = args.get(i + 1) {
                    shot = Some(PathBuf::from(p));
                    i += 1;
                }
            }
            "--minimized" => minimized = true,
            // open straight on an online source: --source google|fontshare|fontsquirrel
            "--source" => {
                if let Some(s) = args.get(i + 1) {
                    std::env::set_var("FONTCRAFT_SOURCE", s);
                    i += 1;
                }
            }
            // open straight on the Font finder page
            "--finder" => picture = Some(PathBuf::new()),
            a if !a.starts_with("--") && app::is_image(std::path::Path::new(a)) => picture = Some(PathBuf::from(a)),
            _ => {}
        }
        i += 1;
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("FontCraft")
        .with_app_id("fontcraft")
        .with_inner_size([1360.0, 840.0])
        .with_min_inner_size([960.0, 560.0])
        .with_drag_and_drop(true)
        .with_icon(icon());
    if shot.is_some() {
        // a picture for a check: don't take the focus or cover what the person is doing
        viewport = viewport.with_active(false);
        #[cfg(windows)]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    #[cfg(target_os = "macos")]
    if shot.is_some() {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        }));
    }
    eframe::run_native(
        "FontCraft",
        options,
        Box::new(move |cc| Ok(Box::new(app::FontCraftApp::new(cc, shot, minimized, picture)))),
    )
}
