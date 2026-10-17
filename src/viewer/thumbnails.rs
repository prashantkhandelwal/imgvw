use eframe::egui::{self, Color32, Vec2};

use super::theme::BACKGROUND;
use super::{MAX_PENDING, Viewer};
use crate::loader::{Kind, Request};

impl Viewer {
    pub(super) fn grid(&mut self, ui: &mut egui::Ui) {
        let gap = 4.0;
        let columns = ((ui.available_width() + gap) / (self.thumbnail_size + gap))
            .floor()
            .max(1.0) as usize;
        let width = ((ui.available_width() - gap * (columns - 1) as f32) / columns as f32).max(1.0);
        let height = width * 0.68;
        let stride = height + gap;
        let rows = self.paths.len().div_ceil(columns);
        let mut scroll = egui::ScrollArea::vertical().id_salt(("grid", self.generation));
        if self.reveal_selected
            && let Some(index) = self.selected
        {
            scroll = scroll.vertical_scroll_offset(
                ((index / columns) as f32 * stride
                    - (ui.available_height() / 2.0 - height / 2.0).max(0.0))
                .max(0.0),
            );
        }
        self.reveal_selected = false;
        scroll.show_viewport(ui, |ui, viewport| {
            ui.set_min_size(Vec2::new(ui.available_width(), rows as f32 * stride));
            let first = (viewport.min.y / stride).floor().max(0.0) as usize;
            let last = ((viewport.max.y / stride).ceil() as usize + 1).min(rows);
            let origin = ui.min_rect().min;
            for row in first..last {
                for column in 0..columns {
                    let index = row * columns + column;
                    if index >= self.paths.len() {
                        break;
                    }
                    let rect = egui::Rect::from_min_size(
                        origin + Vec2::new(column as f32 * (width + gap), row as f32 * stride),
                        Vec2::new(width, height),
                    );
                    self.thumbnail(ui, index, rect, false);
                }
            }
        });
    }

    fn thumbnail(&mut self, ui: &mut egui::Ui, index: usize, rect: egui::Rect, show_name: bool) {
        let response = ui.allocate_rect(rect, egui::Sense::click());
        let active = self.selected == Some(index);
        ui.painter().rect_filled(rect, 0.0, BACKGROUND);
        let image_rect = if show_name {
            egui::Rect::from_min_size(rect.min + Vec2::splat(5.0), Vec2::new(106.0, 69.0))
        } else {
            rect.shrink(2.0)
        };
        if let Some(thumbnail) = self.thumbnails.get_mut(&index) {
            thumbnail.last_used = self.frame;
            let size = thumbnail.texture.size_vec2();
            let fitted = size * (image_rect.width() / size.x).min(image_rect.height() / size.y);
            ui.painter().image(
                thumbnail.texture.id(),
                egui::Rect::from_center_size(image_rect.center(), fitted),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            ui.painter().text(
                image_rect.center(),
                egui::Align2::CENTER_CENTER,
                if self.errors.contains_key(&index) {
                    "Cannot load"
                } else {
                    "Loading..."
                },
                egui::FontId::proportional(12.0),
                Color32::GRAY,
            );
            if !self.errors.contains_key(&index)
                && !self.pending.contains(&index)
                && self.pending.len() < MAX_PENDING
            {
                self.pending.insert(index);
                self.loader.request(Request {
                    generation: self.generation,
                    index,
                    path: self.paths[index].clone(),
                    kind: Kind::Thumbnail,
                });
            }
        }
        if active {
            ui.painter().rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(2.0, Color32::WHITE),
                egui::StrokeKind::Inside,
            );
        }
        let filename = self.paths[index]
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if show_name {
            let text_rect =
                egui::Rect::from_min_size(rect.min + Vec2::new(6.0, 78.0), Vec2::new(104.0, 17.0));
            ui.painter().with_clip_rect(text_rect).text(
                text_rect.left_top(),
                egui::Align2::LEFT_TOP,
                &filename,
                egui::FontId::proportional(11.0),
                Color32::LIGHT_GRAY,
            );
        }
        let response = response.on_hover_text(self.errors.get(&index).map_or_else(
            || self.paths[index].display().to_string(),
            |error| format!("{filename}\n{error}"),
        ));
        if response.clicked() || response.double_clicked() {
            self.select(index);
        }
        if response.double_clicked() {
            self.grid_view = false;
            self.reveal_selected = true;
        }
    }

    pub(super) fn filmstrip(&mut self, context: &egui::Context) {
        egui::TopBottomPanel::bottom("filmstrip")
            .exact_height(155.0)
            .show(context, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.strong("FOLDER");
                    ui.weak(format!("{} images", self.paths.len()));
                    ui.separator();
                    ui.weak("Left / Right to navigate  |  Ctrl+O to open");
                });
                let cell_width = 116.0;
                let spacing = ui.spacing().item_spacing.x;
                let selected = self.selected;
                let reveal = self.reveal_selected;
                self.reveal_selected = false;
                let mut scroll =
                    egui::ScrollArea::horizontal().id_salt(("thumbnails", self.generation));
                if reveal && let Some(index) = selected {
                    let offset = index as f32 * (cell_width + spacing);
                    scroll = scroll.horizontal_scroll_offset(
                        (offset - ui.available_width() / 2.0 + cell_width / 2.0).max(0.0),
                    );
                }
                scroll.show_viewport(ui, |ui, viewport| {
                    let count = self.paths.len();
                    let stride = cell_width + spacing;
                    ui.set_min_size(Vec2::new(count as f32 * stride, 106.0));
                    let first = (viewport.min.x / stride).floor().max(0.0) as usize;
                    let last = ((viewport.max.x / stride).ceil() as usize + 1).min(count);
                    let origin = ui.min_rect().min;
                    for index in first..last {
                        let rect = egui::Rect::from_min_size(
                            origin + Vec2::new(index as f32 * stride, 3.0),
                            Vec2::new(cell_width, 100.0),
                        );
                        self.thumbnail(ui, index, rect, true);
                    }
                });
            });
    }
}
