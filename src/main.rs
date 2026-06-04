// MobiControl API Tester
// Cross-platform GUI for testing SOTI MobiControl REST API calls.

mod api;
mod app;
mod auth;
mod credentials;
mod paths;
mod swagger;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 850.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("MobiControl API Tester"),
        ..Default::default()
    };

    eframe::run_native(
        "MobiControl API Tester",
        options,
        Box::new(|cc| Box::new(app::App::new(cc))),
    )
}
