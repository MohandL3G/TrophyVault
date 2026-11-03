#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bloodborne;
mod data;
mod generator;
mod icons;
mod steam;
mod storage;

use app::PassportApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Achievements Passport Generator")
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([960.0, 640.0])
            .with_active(true),
        ..Default::default()
    };

    eframe::run_native(
        "Achievements Passport Generator",
        native_options,
        Box::new(|cc| Ok(Box::new(PassportApp::new(cc)))),
    )
}
