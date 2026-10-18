use std::path::Path;

use eframe::egui::{self, Color32, FontId, Rect};

pub(super) fn filename(ui: &egui::Ui, image_rect: Rect, path: &Path) {
    let visible = image_rect.intersect(ui.clip_rect());
    if !visible.is_positive() {
        return;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let mut job = egui::text::LayoutJob::simple(
        name.into_owned(),
        FontId::proportional(12.0),
        Color32::WHITE,
        (visible.width() - 12.0).max(0.0),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let painter = ui.painter().with_clip_rect(visible);
    let text = painter.layout_job(job);
    let band = Rect::from_min_max(
        egui::pos2(
            visible.left(),
            (visible.bottom() - text.size().y - 8.0).max(visible.top()),
        ),
        visible.right_bottom(),
    );
    painter.rect_filled(band, 0.0, Color32::from_black_alpha(190));
    painter.galley(
        egui::pos2(band.left() + 6.0, band.center().y - text.size().y / 2.0),
        text,
        Color32::WHITE,
    );
}
