use std::path::Path;

use eframe::egui::{self, Color32, RichText, TextureHandle, Vec2};

use super::Viewer;
use super::theme::{ACCENT, BACKGROUND};
use crate::loader::Metadata;

impl Viewer {
    pub(super) fn central_panel(&mut self, context: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BACKGROUND)
                    .inner_margin(if self.grid_view { 2.0 } else { 20.0 }),
            )
            .show(context, |ui| {
                if self.grid_view && !self.paths.is_empty() {
                    self.grid(ui);
                } else if !self.grid_view
                    && let Some(texture) = self.preview.clone()
                {
                    self.preview_image(ui, &texture);
                } else {
                    self.empty_state(ui, context);
                }
            });
    }

    fn preview_image(&mut self, ui: &mut egui::Ui, texture: &TextureHandle) {
        let available = ui.available_size();
        let size = texture.size_vec2();
        let fitted = size * (available.x / size.x).min(available.y / size.y).min(1.0);
        let zoomed = fitted * (self.preview_zoom / 100.0);
        let can_pan = zoomed.x > available.x || zoomed.y > available.y;
        let scroll = egui::ScrollArea::both()
            .id_salt(("preview_zoom", self.generation, self.selected))
            .drag_to_scroll(false)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (canvas, _) =
                    ui.allocate_exact_size(available.max(zoomed), egui::Sense::hover());
                let image_rect = egui::Rect::from_center_size(canvas.center(), zoomed);
                ui.painter().image(
                    texture.id(),
                    image_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                let response = ui.interact(
                    image_rect.intersect(ui.clip_rect()),
                    ui.id().with("preview_image"),
                    if can_pan {
                        egui::Sense::click_and_drag()
                    } else {
                        egui::Sense::click()
                    },
                );
                if response.hovered() {
                    let delta =
                        ui.input_mut(|input| std::mem::take(&mut input.smooth_scroll_delta.y));
                    if delta != 0.0 {
                        self.preview_zoom =
                            (self.preview_zoom * (delta * 0.002).exp()).clamp(25.0, 400.0);
                        ui.ctx().request_repaint();
                    }
                }
                if can_pan {
                    if response.dragged_by(egui::PointerButton::Primary) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    } else {
                        response.clone().on_hover_cursor(egui::CursorIcon::Grab);
                    }
                }
                response
            });
        if scroll.inner.dragged_by(egui::PointerButton::Primary) {
            let max_offset = (scroll.content_size - scroll.inner_rect.size()).max(Vec2::ZERO);
            let mut state = scroll.state;
            state.offset = (state.offset - scroll.inner.drag_delta())
                .max(Vec2::ZERO)
                .min(max_offset);
            state.store(ui.ctx(), scroll.id);
            ui.ctx().request_repaint();
        }
    }

    fn empty_state(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() / 2.0 - 65.0).max(0.0));
                if let Some(error) = self.folder_error.as_ref().or(self.preview_error.as_ref()) {
                    ui.colored_label(Color32::LIGHT_RED, error);
                } else if self.scanning || self.selected.is_some() {
                    ui.spinner();
                    ui.label("Loading...");
                } else {
                    ui.heading(if self.folder.is_some() {
                        "No images in this folder"
                    } else {
                        "A little space for your images."
                    });
                    ui.weak("JPEG, PNG, WebP, GIF, TIFF, BMP and more");
                }
                ui.add_space(14.0);
                if ui.button("Choose a folder").clicked() {
                    self.pick_folder(context);
                }
            });
        });
    }

    pub(super) fn properties(&self, context: &egui::Context) {
        egui::SidePanel::right("properties")
            .resizable(true)
            .default_width(260.0)
            .width_range(210.0..=420.0)
            .frame(egui::Frame::side_top_panel(&context.style()).fill(BACKGROUND))
            .show(context, |ui| {
                ui.add_space(12.0);
                egui::ScrollArea::vertical().show(ui, |ui| self.image_details(ui));
            });
    }

    fn image_details(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("IMAGE DETAILS").strong().color(ACCENT));
        ui.add_space(10.0);
        let Some(index) = self.selected else {
            ui.weak("Select an image to see its properties and EXIF tags.");
            return;
        };
        let path = &self.paths[index];
        ui.label(RichText::new(path.file_name().unwrap_or_default().to_string_lossy()).strong());
        ui.add(egui::Label::new(path.display().to_string()).wrap());
        ui.add_space(8.0);
        if let Some(metadata) = &self.metadata {
            metadata_details(ui, path, metadata);
        } else if let Some(error) = &self.preview_error {
            ui.colored_label(Color32::LIGHT_RED, error);
        } else {
            ui.spinner();
        }
    }
}

fn metadata_details(ui: &mut egui::Ui, path: &Path, metadata: &Metadata) {
    ui.label(format!("{} x {} px", metadata.width, metadata.height));
    ui.label(format!(
        "{:.2} MB  ({} bytes)",
        metadata.bytes as f64 / 1_000_000.0,
        metadata.bytes
    ));
    ui.label(format!(
        "Format: {}",
        path.extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_uppercase()
    ));
    ui.separator();
    egui::CollapsingHeader::new(RichText::new("EXIF").strong())
        .default_open(false)
        .show(ui, |ui| {
            if let Some(error) = &metadata.exif_error {
                ui.colored_label(Color32::LIGHT_RED, error);
            } else if metadata.fields.is_empty() {
                ui.weak("No EXIF metadata in this image.");
            }
            for (name, value) in &metadata.fields {
                ui.add_space(5.0);
                ui.weak(name);
                ui.add(egui::Label::new(value).wrap().selectable(true));
            }
        });
}
