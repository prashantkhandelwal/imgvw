mod controls;
mod preferences;
mod preview;
mod theme;
mod thumbnails;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui::{self, TextureHandle};

use crate::loader::{Decoded, Event, Kind, Loader, Metadata, Request};
use crate::settings::{DefaultView, Settings};

const MAX_THUMBNAILS: usize = 256;
const MAX_PENDING: usize = 48;
const MAX_TEXTURE_UPLOADS_PER_FRAME: usize = 8;

struct Thumbnail {
    texture: TextureHandle,
    last_used: u64,
}

pub(crate) struct Viewer {
    loader: Loader,
    generation: u64,
    folder: Option<PathBuf>,
    paths: Vec<PathBuf>,
    selected: Option<usize>,
    scanning: bool,
    picker: Option<mpsc::Receiver<Option<PathBuf>>>,
    thumbnails: HashMap<usize, Thumbnail>,
    pending: HashSet<usize>,
    errors: HashMap<usize, String>,
    preview: Option<TextureHandle>,
    preview_error: Option<String>,
    metadata: Option<Metadata>,
    folder_error: Option<String>,
    frame: u64,
    reveal_selected: bool,
    preferences_open: bool,
    settings: Settings,
    settings_path: Option<PathBuf>,
    preferences_draft: Settings,
    preferences_picker: Option<mpsc::Receiver<Option<PathBuf>>>,
    preferences_error: Option<String>,
    preferences_saved: bool,
    grid_view: bool,
    thumbnail_size: f32,
    preview_zoom: f32,
}

impl Viewer {
    pub(crate) fn new(context: egui::Context) -> Self {
        theme::apply(&context);
        Self {
            loader: Loader::new(context),
            generation: 0,
            folder: None,
            paths: Vec::new(),
            selected: None,
            scanning: false,
            picker: None,
            thumbnails: HashMap::new(),
            pending: HashSet::new(),
            errors: HashMap::new(),
            preview: None,
            preview_error: None,
            metadata: None,
            folder_error: None,
            frame: 0,
            reveal_selected: false,
            preferences_open: false,
            settings: Settings::default(),
            settings_path: None,
            preferences_draft: Settings::default(),
            preferences_picker: None,
            preferences_error: None,
            preferences_saved: false,
            grid_view: true,
            thumbnail_size: 144.0,
            preview_zoom: 100.0,
        }
    }

    fn pick_folder(&mut self, context: &egui::Context) {
        if self.picker_open() {
            return;
        }
        self.picker = Some(Self::folder_dialog(
            context,
            "Open image folder",
            &self.settings.default_folder,
        ));
    }

    fn picker_open(&self) -> bool {
        self.picker.is_some() || self.preferences_picker.is_some()
    }

    fn folder_dialog(
        context: &egui::Context,
        title: &'static str,
        initial: &str,
    ) -> mpsc::Receiver<Option<PathBuf>> {
        let (sender, receiver) = mpsc::channel();
        let context = context.clone();
        let initial = PathBuf::from(initial);
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new().set_title(title);
            if initial.is_dir() {
                dialog = dialog.set_directory(initial);
            }
            let folder = dialog.pick_folder();
            let _ = sender.send(folder);
            context.request_repaint();
        });
        receiver
    }

    fn open_folder(&mut self, path: PathBuf, context: &egui::Context) {
        self.generation = self.loader.open_folder(path.clone(), context.clone());
        self.folder = Some(path);
        self.paths.clear();
        self.selected = None;
        self.thumbnails.clear();
        self.pending.clear();
        self.errors.clear();
        self.preview = None;
        self.preview_error = None;
        self.metadata = None;
        self.folder_error = None;
        self.scanning = true;
        self.grid_view = self.settings.default_view == DefaultView::Grid;
    }

    fn select(&mut self, index: usize) {
        if self.selected == Some(index) || index >= self.paths.len() {
            return;
        }
        self.selected = Some(index);
        self.preview = None;
        self.preview_error = None;
        self.metadata = None;
        self.reveal_selected = true;
        self.loader.request(Request {
            generation: self.generation,
            index,
            path: self.paths[index].clone(),
            kind: Kind::Preview,
        });
    }

    fn navigate(&mut self, forward: bool) {
        if let Some(index) = next_index(self.selected, self.paths.len(), forward) {
            self.select(index);
        }
    }

    fn receive(&mut self, context: &egui::Context) {
        self.receive_preferences_picker();
        self.receive_folder_picker(context);
        self.receive_loader_events(context);
        self.trim_thumbnail_cache();
    }

    fn receive_folder_picker(&mut self, context: &egui::Context) {
        if let Some(receiver) = &self.picker {
            match receiver.try_recv() {
                Ok(folder) => {
                    self.picker = None;
                    if let Some(path) = folder {
                        self.open_folder(path, context);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.picker = None;
                    self.folder_error = Some("The folder picker closed unexpectedly.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    fn receive_loader_events(&mut self, context: &egui::Context) {
        // Bound texture uploads per frame so decoding cannot monopolize the UI.
        for _ in 0..MAX_TEXTURE_UPLOADS_PER_FRAME {
            let Ok(event) = self.loader.events.try_recv() else {
                break;
            };
            match event {
                Event::Folder {
                    generation,
                    path,
                    result,
                } if generation == self.generation => {
                    self.scanning = false;
                    self.folder = Some(path);
                    match result {
                        Ok(paths) => {
                            self.paths = paths;
                            if !self.paths.is_empty() {
                                self.select(0);
                            }
                        }
                        Err(error) => self.folder_error = Some(error),
                    }
                }
                Event::Image { request, result } if request.generation == self.generation => {
                    if request.kind == Kind::Thumbnail {
                        self.pending.remove(&request.index);
                    } else if self.selected != Some(request.index) {
                        continue;
                    }
                    match result {
                        Ok(decoded) => self.upload_image(context, request, decoded),
                        Err(error) => {
                            if request.kind == Kind::Preview {
                                self.preview_error = Some(error);
                            } else {
                                self.errors.insert(request.index, error);
                            }
                        }
                    }
                }
                _ => {}
            }
            context.request_repaint();
        }
    }

    fn upload_image(&mut self, context: &egui::Context, request: Request, decoded: Decoded) {
        let size = [
            decoded.pixels.width() as usize,
            decoded.pixels.height() as usize,
        ];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, decoded.pixels.as_raw());
        let texture = context.load_texture(
            format!("{}-{}-{:?}", self.generation, request.index, request.kind),
            image,
            egui::TextureOptions::LINEAR,
        );
        if request.kind == Kind::Preview {
            self.preview = Some(texture);
            self.metadata = decoded.metadata;
        } else {
            self.thumbnails.insert(
                request.index,
                Thumbnail {
                    texture,
                    last_used: self.frame,
                },
            );
        }
    }

    fn trim_thumbnail_cache(&mut self) {
        while self.thumbnails.len() > MAX_THUMBNAILS {
            let oldest = self
                .thumbnails
                .iter()
                .min_by_key(|(_, thumbnail)| thumbnail.last_used)
                .map(|(index, _)| *index)
                .expect("nonempty thumbnail cache");
            self.thumbnails.remove(&oldest);
        }
    }

    fn show(&mut self, context: &egui::Context) {
        self.frame += 1;
        self.receive(context);
        self.handle_shortcuts(context);
        self.toolbar(context);
        if !self.grid_view {
            self.filmstrip(context);
            self.properties(context);
        }
        self.central_panel(context);
    }

    fn handle_shortcuts(&mut self, context: &egui::Context) {
        if !self.picker_open() && !self.preferences_open && !context.wants_keyboard_input() {
            if context.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.grid_view = true;
                self.reveal_selected = true;
            }
            if context.input(|input| input.key_pressed(egui::Key::Enter)) && self.selected.is_some()
            {
                self.grid_view = false;
                self.reveal_selected = true;
            }
            if context.input(|input| input.key_pressed(egui::Key::ArrowRight)) {
                self.navigate(true);
            }
            if context.input(|input| input.key_pressed(egui::Key::ArrowLeft)) {
                self.navigate(false);
            }
        }
        if context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.pick_folder(context);
        }
    }
}

impl eframe::App for Viewer {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.show(context);
    }
}

fn next_index(selected: Option<usize>, count: usize, forward: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match selected {
        None => 0,
        Some(index) if forward => (index + 1) % count,
        Some(0) => count - 1,
        Some(index) => index - 1,
    })
}

#[cfg(test)]
mod tests;
