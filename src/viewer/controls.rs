use eframe::egui::{self, RichText, Vec2};

use super::Viewer;
use super::theme::{BACKGROUND, MENU_FONT_SIZE};

impl Viewer {
    pub(super) fn toolbar(&mut self, context: &egui::Context) {
        self.menu_bar(context);
        self.preferences(context);
        self.status_bar(context);
        self.folder_heading(context);
    }

    fn menu_bar(&mut self, context: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar")
            .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(6.0))
            .show(context, |ui| {
                ui.style_mut().text_styles.insert(
                    egui::TextStyle::Button,
                    egui::FontId::proportional(MENU_FONT_SIZE),
                );
                ui.style_mut().text_styles.insert(
                    egui::TextStyle::Body,
                    egui::FontId::proportional(MENU_FONT_SIZE),
                );
                ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.spacing_mut().interact_size.y = 28.0;
                egui::menu::bar(ui, |ui| {
                    let menu_font = egui::TextStyle::Button.resolve(ui.style());
                    ui.menu_button("File", |ui| {
                        ui.style_mut().override_font_id = Some(menu_font);
                        if ui
                            .add_enabled(!self.picker_open(), egui::Button::new("Open Folder(s)"))
                            .clicked()
                        {
                            self.pick_folder(context);
                            ui.close_menu();
                        }
                        if ui.button("Preferences").clicked() {
                            if !self.preferences_open {
                                self.preferences_draft = self.settings.clone();
                                self.preferences_saved = false;
                            }
                            self.preferences_open = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Exit").clicked() {
                            context.send_viewport_cmd(egui::ViewportCommand::Close);
                            ui.close_menu();
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, context: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(8.0))
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new("Photos").selected(self.grid_view))
                        .clicked()
                    {
                        self.grid_view = true;
                        self.reveal_selected = true;
                    }
                    if ui
                        .add_enabled(
                            self.selected.is_some(),
                            egui::Button::new("Preview").selected(!self.grid_view),
                        )
                        .clicked()
                    {
                        self.grid_view = false;
                        self.reveal_selected = true;
                    }
                    ui.separator();
                    ui.weak("Ctrl+O: open folder");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !self.grid_view {
                            ui.add(
                                egui::Slider::new(&mut self.preview_zoom, 25.0..=400.0)
                                    .suffix("%")
                                    .text("Zoom"),
                            )
                            .on_hover_text("100% uses the fitted preview size. Scroll to explore enlarged images.");
                        } else if ui
                            .add(
                                egui::Slider::new(&mut self.thumbnail_size, 80.0..=240.0)
                                    .show_value(false)
                                    .text("Thumbnail size"),
                            )
                            .changed()
                        {
                            self.reveal_selected = true;
                        }
                    });
                });
            });
    }

    fn folder_heading(&self, context: &egui::Context) {
        egui::TopBottomPanel::top("folder_heading").show(context, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if self.scanning {
                    ui.spinner();
                    ui.label(format!(
                        "Reading recursively: {} files checked, {} images found",
                        self.scan_progress.files_read, self.scan_progress.images_found
                    ));
                } else if let Some(folder) = &self.folder {
                    let name = folder
                        .file_name()
                        .unwrap_or(folder.as_os_str())
                        .to_string_lossy();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let count = self.paths.len();
                        ui.weak(format!(
                            "{count} {}",
                            if count == 1 { "image" } else { "images" }
                        ));
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(RichText::new(name).strong()).truncate())
                                .on_hover_text(folder.display().to_string());
                        });
                    });
                } else {
                    ui.weak("Open a folder to browse your photos");
                }
            });
            ui.add_space(6.0);
        });
    }
}
