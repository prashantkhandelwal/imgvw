use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui::{self, Color32};

use super::Viewer;
use crate::settings::{self, DefaultView};

impl Viewer {
    pub(super) fn receive_preferences_picker(&mut self) {
        let Some(receiver) = &self.preferences_picker else {
            return;
        };
        match receiver.try_recv() {
            Ok(folder) => {
                self.preferences_picker = None;
                if self.preferences_open
                    && let Some(path) = folder
                {
                    match path.into_os_string().into_string() {
                        Ok(path) => self.preferences_draft.default_folder = path,
                        Err(_) => {
                            self.preferences_error =
                                Some("The selected folder path cannot be displayed as text.".into())
                        }
                    }
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.preferences_picker = None;
                self.preferences_error = Some("The folder picker closed unexpectedly.".into());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    pub(crate) fn restore_settings(&mut self, context: &egui::Context) {
        match settings::path() {
            Ok(path) => {
                self.load_settings(&path, context);
            }
            Err(error) => {
                self.preferences_error = Some(error);
                self.preferences_open = true;
            }
        }
    }

    pub(super) fn load_settings(&mut self, path: &std::path::Path, context: &egui::Context) {
        self.settings_path = Some(path.to_owned());
        match settings::load(path) {
            Ok(settings) => {
                self.settings = settings;
                self.preferences_draft = self.settings.clone();
                self.grid_view = self.settings.default_view == DefaultView::Grid;
                if !self.settings.default_folder.is_empty() {
                    self.open_folder(PathBuf::from(&self.settings.default_folder), context);
                }
            }
            Err(error) => {
                self.preferences_error = Some(error);
                self.preferences_open = true;
            }
        }
    }

    pub(super) fn save_preferences(&mut self) -> bool {
        let result = self
            .settings_path
            .as_ref()
            .ok_or_else(|| "The user's settings directory is unavailable.".to_string())
            .and_then(|path| settings::save(path, &self.preferences_draft));
        match result {
            Ok(()) => {
                if self.preferences_draft.default_view != self.settings.default_view {
                    self.grid_view = self.preferences_draft.default_view == DefaultView::Grid;
                    self.reveal_selected = true;
                }
                self.settings = self.preferences_draft.clone();
                self.preferences_error = None;
                self.preferences_saved = true;
                true
            }
            Err(error) => {
                self.preferences_error = Some(error);
                self.preferences_saved = false;
                false
            }
        }
    }

    pub(super) fn preferences(&mut self, context: &egui::Context) {
        let mut open = self.preferences_open;
        let mut save = false;
        let mut close_after_save = false;
        let mut window = egui::Window::new("Preferences")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .min_size(egui::vec2(520.0, 200.0))
            .pivot(egui::Align2::CENTER_CENTER);
        if self.preferences_open && !self.preferences_was_open {
            window = window.current_pos(context.screen_rect().center());
        }
        window.show(context, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.label("Default folder location");
            ui.horizontal(|ui| {
                let browse_width = 80.0;
                let field_width =
                    (ui.available_width() - browse_width - ui.spacing().item_spacing.x).max(0.0);
                ui.add_sized(
                    egui::vec2(field_width, ui.spacing().interact_size.y),
                    egui::TextEdit::singleline(&mut self.preferences_draft.default_folder)
                        .hint_text("Choose a folder (optional)")
                        .desired_width(f32::INFINITY),
                );
                if ui
                    .add_enabled(
                        !self.picker_open(),
                        egui::Button::new("Browse...")
                            .min_size(egui::vec2(browse_width, ui.spacing().interact_size.y)),
                    )
                    .clicked()
                {
                    self.preferences_picker = Some(Self::folder_dialog(
                        context,
                        "Choose default folder",
                        &self.preferences_draft.default_folder,
                    ));
                }
            });
            ui.weak("Opened automatically when viewr starts. Leave empty for no folder.");
            ui.checkbox(
                &mut self.preferences_draft.include_hidden,
                "Search hidden files and folders",
            )
            .on_hover_text(
                "Applies when opening a folder. Hidden images that cannot be decoded are skipped.",
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label("Default view");
                egui::ComboBox::from_id_salt("default_view")
                    .selected_text(self.preferences_draft.default_view.label())
                    .show_ui(ui, |ui| {
                        for view in [DefaultView::Grid, DefaultView::Preview] {
                            ui.selectable_value(
                                &mut self.preferences_draft.default_view,
                                view,
                                view.label(),
                            );
                        }
                    });
            });
            ui.checkbox(
                &mut self.preferences_draft.retain_zoom,
                "Retain zoom percentage between images",
            )
            .on_hover_text(
                "When unchecked, selecting a different image resets zoom to 100% (fitted).",
            );
            if let Some(error) = &self.preferences_error {
                ui.add_space(10.0);
                ui.colored_label(Color32::LIGHT_RED, error);
            } else if self.preferences_saved && self.preferences_draft == self.settings {
                ui.add_space(10.0);
                ui.label("Settings saved.");
            }
            let button_size = egui::vec2(80.0, 28.0);
            ui.add_space(
                (ui.available_height() - button_size.y - ui.spacing().item_spacing.y).max(16.0),
            );
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), button_size.y),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.add_enabled_ui(!self.picker_open(), |ui| {
                        if ui.add_sized(button_size, egui::Button::new("Ok")).clicked() {
                            save = true;
                            close_after_save = true;
                        }
                        if ui
                            .add_sized(button_size, egui::Button::new("Save"))
                            .clicked()
                        {
                            save = true;
                        }
                    });
                },
            );
        });
        self.preferences_open = open;
        if save && self.save_preferences() && close_after_save {
            self.preferences_open = false;
        }
        self.preferences_was_open = self.preferences_open;
    }
}
