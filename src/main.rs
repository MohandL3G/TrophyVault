#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bloodborne;
mod data;
mod generator;
mod icons;
mod steam;
mod storage;

use app::PassportApp;
use std::sync::Arc;

fn load_app_icon() -> Option<Arc<eframe::egui::IconData>> {
    let icon_bytes = include_bytes!("../assets/trophyvault-icon-256.png");
    if let Ok(img) = image::load_from_memory_with_format(icon_bytes, image::ImageFormat::Png) {
        let rgba = img.into_rgba8();
        let (width, height) = rgba.dimensions();
        Some(Arc::new(eframe::egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        }))
    } else {
        None
    }
}

fn main() -> eframe::Result<()> {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("TrophyVault")
        .with_inner_size([1280.0, 860.0])
        .with_min_inner_size([960.0, 640.0])
        .with_active(true);

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "TrophyVault",
        native_options,
        Box::new(|cc| Ok(Box::new(PassportApp::new(cc)))),
    )
}
