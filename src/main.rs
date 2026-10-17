mod loader;
mod settings;
mod viewer;

use eframe::egui;
use viewer::Viewer;

fn main() -> eframe::Result {
    eframe::run_native(
        "imgvw - Image Viewer",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 820.0])
                .with_min_inner_size([760.0, 480.0]),
            ..Default::default()
        },
        Box::new(|cc| {
            let mut viewer = Viewer::new(cc.egui_ctx.clone());
            viewer.restore_settings(&cc.egui_ctx);
            Ok(Box::new(viewer))
        }),
    )
}
