#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod export;
mod host_info;
mod oui_registry;
mod oui_store;
mod profile;
mod scanner;
mod ui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([880.0, 560.0])
            .with_title("IP / PORT Scanner"),
        ..Default::default()
    };

    eframe::run_native(
        "IP / PORT Scanner",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
