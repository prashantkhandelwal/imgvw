mod loader;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui::{self, Color32, RichText, TextureHandle, Vec2};
use loader::{Event, Kind, Loader, Metadata, Request};

const ACCENT: Color32 = Color32::from_rgb(239, 171, 86);
const BACKGROUND: Color32 = Color32::BLACK;
const CHROME: Color32 = Color32::from_rgb(43, 43, 43);
const MENU_FONT_SIZE: f32 = 16.0;
const MAX_THUMBNAILS: usize = 256;
const MAX_PENDING: usize = 48;

struct Thumbnail {
    texture: TextureHandle,
    last_used: u64,
}

struct Viewer {
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
    grid_view: bool,
    thumbnail_size: f32,
}

impl Viewer {
    fn new(context: egui::Context) -> Self {
        context.set_theme(egui::Theme::Dark);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BACKGROUND;
        visuals.window_fill = BACKGROUND;
        visuals.selection.bg_fill = ACCENT.gamma_multiply(0.3);
        visuals.selection.stroke.color = ACCENT;
        context.set_visuals(visuals);
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
            grid_view: true,
            thumbnail_size: 144.0,
        }
    }

    fn pick_folder(&mut self, context: &egui::Context) {
        if self.picker.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.picker = Some(receiver);
        let context = context.clone();
        std::thread::spawn(move || {
            let folder = rfd::FileDialog::new()
                .set_title("Open image folder")
                .pick_folder();
            let _ = sender.send(folder);
            context.request_repaint();
        });
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
        self.grid_view = true;
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
        // Bound texture uploads per frame so decoding cannot monopolize the UI.
        for _ in 0..8 {
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
                        Ok(decoded) => {
                            let size = [
                                decoded.pixels.width() as usize,
                                decoded.pixels.height() as usize,
                            ];
                            let image = egui::ColorImage::from_rgba_unmultiplied(
                                size,
                                decoded.pixels.as_raw(),
                            );
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

    fn toolbar(&mut self, context: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar")
            .frame(egui::Frame::new().fill(CHROME).inner_margin(6.0))
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
                    ui.menu_button("File", |ui| {
                        if ui
                            .add_enabled(self.picker.is_none(), egui::Button::new("Open Folder(s)"))
                            .clicked()
                        {
                            self.pick_folder(context);
                            ui.close_menu();
                        }
                        if ui.button("Preferences").clicked() {
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
        egui::Window::new("Preferences")
            .open(&mut self.preferences_open)
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Default location");
                ui.weak("The default folder location is not configurable yet.");
            });
        egui::TopBottomPanel::top("toolbar").show(context, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("All Photos").strong().size(18.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!("{} photos", self.paths.len()));
                });
            });
            ui.add_space(6.0);
        });
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::new().fill(CHROME).inner_margin(8.0))
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.grid_view, "Photos").clicked() {
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
                        if ui
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
        egui::TopBottomPanel::top("folder_heading").show(context, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if self.scanning {
                    ui.spinner();
                    ui.label("Reading folder...");
                } else if let Some(folder) = &self.folder {
                    let name = folder
                        .file_name()
                        .unwrap_or(folder.as_os_str())
                        .to_string_lossy();
                    ui.add(egui::Label::new(RichText::new(name).strong()).truncate())
                        .on_hover_text(folder.display().to_string());
                } else {
                    ui.weak("Open a folder to browse your photos");
                }
            });
            ui.add_space(6.0);
        });
    }

    fn grid(&mut self, ui: &mut egui::Ui) {
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

    fn thumbnail(&mut self, ui: &mut egui::Ui, index: usize, rect: egui::Rect, name: bool) {
        let response = ui.allocate_rect(rect, egui::Sense::click());
        let active = self.selected == Some(index);
        ui.painter().rect_filled(rect, 0.0, CHROME);
        let image_rect = if name {
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
        if name {
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

    fn filmstrip(&mut self, context: &egui::Context) {
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

    fn properties(&self, context: &egui::Context) {
        egui::SidePanel::right("properties")
            .resizable(true)
            .default_width(260.0)
            .width_range(210.0..=420.0)
            .show(context, |ui| {
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(12.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), ui.available_height().min(540.0)),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(ui.available_height().min(540.0))
                                .show(ui, |ui| {
                                    ui.label(RichText::new("IMAGE DETAILS").strong().color(ACCENT));
                                    ui.add_space(10.0);
                                    if let Some(index) = self.selected {
                                        let path = &self.paths[index];
                                        ui.label(
                                            RichText::new(
                                                path.file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy(),
                                            )
                                            .strong(),
                                        );
                                        ui.add(egui::Label::new(path.display().to_string()).wrap());
                                        ui.add_space(8.0);
                                        if let Some(metadata) = &self.metadata {
                                            ui.label(format!(
                                                "{} x {} px",
                                                metadata.width, metadata.height
                                            ));
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
                                            egui::CollapsingHeader::new(
                                                RichText::new("EXIF").strong(),
                                            )
                                            .default_open(false)
                                            .show(
                                                ui,
                                                |ui| {
                                                    if let Some(error) = &metadata.exif_error {
                                                        ui.colored_label(Color32::LIGHT_RED, error);
                                                    } else if metadata.fields.is_empty() {
                                                        ui.weak("No EXIF metadata in this image.");
                                                    }
                                                    for (name, value) in &metadata.fields {
                                                        ui.add_space(5.0);
                                                        ui.weak(name);
                                                        ui.add(
                                                            egui::Label::new(value)
                                                                .wrap()
                                                                .selectable(true),
                                                        );
                                                    }
                                                },
                                            );
                                        } else if let Some(error) = &self.preview_error {
                                            ui.colored_label(Color32::LIGHT_RED, error);
                                        } else {
                                            ui.spinner();
                                        }
                                    } else {
                                        ui.weak(
                                            "Select an image to see its properties and EXIF tags.",
                                        );
                                    }
                                });
                        },
                    );
                });
            });
    }
}

impl Viewer {
    fn show(&mut self, context: &egui::Context) {
        self.frame += 1;
        self.receive(context);
        if self.picker.is_none() && !context.wants_keyboard_input() {
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
        self.toolbar(context);
        if !self.grid_view {
            self.filmstrip(context);
            self.properties(context);
        }
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
                    && let Some(texture) = &self.preview
                {
                    let available = ui.available_size();
                    let size = texture.size_vec2();
                    let fitted = size * (available.x / size.x).min(available.y / size.y).min(1.0);
                    ui.centered_and_justified(|ui| {
                        ui.add(egui::Image::new(texture).fit_to_exact_size(fitted));
                    });
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space((ui.available_height() / 2.0 - 65.0).max(0.0));
                            if let Some(error) =
                                self.folder_error.as_ref().or(self.preview_error.as_ref())
                            {
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
            });
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

fn main() -> eframe::Result {
    eframe::run_native(
        "imgvw - Image Viewer",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 820.0])
                .with_min_inner_size([760.0, 480.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(Viewer::new(cc.egui_ctx.clone())))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_wraps_and_handles_empty_or_single_image() {
        assert_eq!(next_index(None, 0, true), None);
        assert_eq!(next_index(None, 3, false), Some(0));
        assert_eq!(next_index(Some(0), 3, false), Some(2));
        assert_eq!(next_index(Some(2), 3, true), Some(0));
        assert_eq!(next_index(Some(1), 3, false), Some(0));
        assert_eq!(next_index(Some(0), 1, true), Some(0));
        assert_eq!(next_index(Some(0), 1, false), Some(0));
    }

    fn render(
        viewer: &mut Viewer,
        context: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1280.0, 820.0),
            )),
            events,
            ..Default::default()
        };
        let output = context.run(input, |context| viewer.show(context));
        assert!(!output.shapes.is_empty());
        output
    }

    fn wait_until(viewer: &mut Viewer, context: &egui::Context, ready: impl Fn(&Viewer) -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            render(viewer, context, Vec::new());
            if ready(viewer) {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "UI loading timed out");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn text_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + Vec2::splat(5.0))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Missing menu text: {label}"))
    }

    fn click(viewer: &mut Viewer, context: &egui::Context, position: egui::Pos2) {
        for pressed in [true, false] {
            render(
                viewer,
                context,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    fn menu_bar_has_larger_text_without_navigation_buttons() {
        let context = egui::Context::default();
        let mut viewer = Viewer::new(context.clone());
        render(&mut viewer, &context, Vec::new());
        let output = render(&mut viewer, &context, Vec::new());
        text_position(&output, "File");
        assert!(!output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text)
                if matches!(text.galley.job.text.as_str(), "\u{2190}" | "\u{2192}" | "Back" | "Forward"))
        }));
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text)
            if text.galley.job.text == "File"
            && text.galley.job.sections.iter().all(|section| {
                section.format.font_id.size == MENU_FONT_SIZE
            }))
        }));
    }

    #[test]
    fn file_menu_opens_preferences_and_requests_exit() {
        let context = egui::Context::default();
        let mut viewer = Viewer::new(context.clone());
        render(&mut viewer, &context, Vec::new());
        let output = render(&mut viewer, &context, Vec::new());
        let file_position = text_position(&output, "File");
        click(&mut viewer, &context, file_position);
        let output = render(&mut viewer, &context, Vec::new());
        text_position(&output, "Open Folder(s)");
        text_position(&output, "Exit");
        click(&mut viewer, &context, text_position(&output, "Preferences"));
        assert!(viewer.preferences_open);
        let output = render(&mut viewer, &context, Vec::new());
        text_position(&output, "Default location");

        click(&mut viewer, &context, file_position);
        let output = render(&mut viewer, &context, Vec::new());
        let exit_position = text_position(&output, "Exit");
        let mut closed = false;
        for pressed in [true, false] {
            let output = render(
                &mut viewer,
                &context,
                vec![
                    egui::Event::PointerMoved(exit_position),
                    egui::Event::PointerButton {
                        pos: exit_position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            closed |= output
                .viewport_output
                .values()
                .any(|viewport| viewport.commands.contains(&egui::ViewportCommand::Close));
        }
        assert!(closed, "Exit should request that the native window close");
    }

    #[test]
    fn grid_uses_dark_chrome_and_black_canvas() {
        assert_eq!(BACKGROUND, Color32::from_rgb(0, 0, 0));
        let context = egui::Context::default();
        let mut viewer = Viewer::new(context.clone());
        render(&mut viewer, &context, Vec::new());
        let output = render(&mut viewer, &context, Vec::new());
        assert_eq!(context.style().visuals.panel_fill, BACKGROUND);
        assert_eq!(context.style().visuals.window_fill, BACKGROUND);
        for (point, color) in [
            (egui::pos2(5.0, 5.0), CHROME),
            (egui::pos2(5.0, 810.0), CHROME),
            (egui::pos2(1270.0, 170.0), BACKGROUND),
            (egui::pos2(5.0, 170.0), BACKGROUND),
        ] {
            assert!(
                output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(rect)
                    if rect.rect.contains(point) && rect.fill == color)
                }),
                "Missing matching panel background at {point:?}"
            );
        }
    }

    #[test]
    fn grid_size_control_and_preview_round_trip() {
        fn selected_rect(output: &egui::FullOutput) -> egui::Rect {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.stroke.color == Color32::WHITE && rect.stroke.width == 2.0 =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("Selected thumbnail should have a white outline")
        }

        let folder = tempfile::tempdir().unwrap();
        image::RgbaImage::new(40, 20)
            .save(folder.path().join("photo.png"))
            .unwrap();
        let context = egui::Context::default();
        let mut viewer = Viewer::new(context.clone());
        assert!(viewer.grid_view);
        viewer.open_folder(folder.path().to_owned(), &context);
        wait_until(&mut viewer, &context, |viewer| {
            viewer.preview.is_some() && viewer.thumbnails.contains_key(&0)
        });
        let output = render(&mut viewer, &context, Vec::new());
        let initial = selected_rect(&output);
        assert!(initial.top() < 160.0);
        assert!(initial.width() > initial.height());
        let slider_position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Circle(circle) if circle.center.y > 780.0 => {
                    Some(circle.center + Vec2::new(20.0, 0.0))
                }
                _ => None,
            })
            .expect("Thumbnail size slider handle should be visible");
        click(&mut viewer, &context, slider_position);
        assert_ne!(
            viewer.thumbnail_size, 144.0,
            "Slider must change thumbnail size"
        );
        let output = render(&mut viewer, &context, Vec::new());
        let resized = selected_rect(&output);
        assert_ne!(resized.width(), initial.width());
        click(&mut viewer, &context, resized.center());
        click(&mut viewer, &context, resized.center());
        assert!(!viewer.grid_view, "Double-click should open preview");
        let output = render(&mut viewer, &context, Vec::new());
        text_position(&output, "IMAGE DETAILS");
        text_position(&output, "40 x 20 px");
        render(
            &mut viewer,
            &context,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(viewer.grid_view);
        let output = render(&mut viewer, &context, Vec::new());
        assert_eq!(selected_rect(&output).width(), resized.width());
    }

    #[test]
    fn system_theme_cannot_override_the_black_panels() {
        let context = egui::Context::default();
        let mut viewer = Viewer::new(context.clone());
        for theme in [egui::Theme::Light, egui::Theme::Dark, egui::Theme::Light] {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1280.0, 820.0),
                )),
                system_theme: Some(theme),
                ..Default::default()
            };
            let _ = context.run(input.clone(), |context| viewer.show(context));
            let output = context.run(input, |context| viewer.show(context));
            assert_eq!(context.theme(), egui::Theme::Dark);
            assert_eq!(context.style().visuals.panel_fill, Color32::BLACK);
            assert_eq!(context.style().visuals.window_fill, Color32::BLACK);
            for (point, color) in [
                (egui::pos2(5.0, 5.0), CHROME),
                (egui::pos2(5.0, 810.0), CHROME),
                (egui::pos2(1270.0, 170.0), BACKGROUND),
                (egui::pos2(5.0, 170.0), BACKGROUND),
            ] {
                assert!(
                    output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, egui::Shape::Rect(rect)
                        if rect.rect.contains(point) && rect.fill == color)
                    }),
                    "System theme {theme:?} changed panel background at {point:?}"
                );
            }
        }
    }

    #[test]
    fn exif_details_are_hidden_until_clicked_and_can_be_collapsed() {
        for (fields, exif_error, expected) in [
            (vec![("Make".into(), "Canon".into())], None, "Canon"),
            (Vec::new(), None, "No EXIF metadata in this image."),
            (
                Vec::new(),
                Some("EXIF unavailable: invalid data".into()),
                "EXIF unavailable: invalid data",
            ),
        ] {
            let context = egui::Context::default();
            let mut viewer = Viewer::new(context.clone());
            viewer.grid_view = false;
            context.style_mut(|style| style.animation_time = 0.0);
            viewer.paths = vec![PathBuf::from("test.png")];
            viewer.selected = Some(0);
            viewer.metadata = Some(Metadata {
                width: 40,
                height: 20,
                bytes: 100,
                fields,
                exif_error,
            });
            let text_position = |output: &egui::FullOutput, label: &str| {
                output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == label => Some(text.pos),
                    _ => None,
                })
            };
            let output = render(&mut viewer, &context, Vec::new());
            assert!(text_position(&output, "40 x 20 px").is_some());
            assert!(text_position(&output, expected).is_none());
            let position = text_position(&output, "EXIF").unwrap() + Vec2::new(5.0, 5.0);
            for open in [true, false] {
                for pressed in [true, false] {
                    render(
                        &mut viewer,
                        &context,
                        vec![
                            egui::Event::PointerMoved(position),
                            egui::Event::PointerButton {
                                pos: position,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                    );
                }
                let output = render(&mut viewer, &context, Vec::new());
                assert_eq!(text_position(&output, expected).is_some(), open);
                assert!(text_position(&output, "40 x 20 px").is_some());
            }
        }
    }

    #[test]
    fn ui_loads_navigates_and_discards_results_from_previous_folder() {
        let folder = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        image::RgbaImage::new(40, 20)
            .save(folder.path().join("a.png"))
            .unwrap();
        image::RgbaImage::new(60, 30)
            .save(folder.path().join("b.png"))
            .unwrap();
        image::RgbaImage::new(12, 24)
            .save(other.path().join("new.png"))
            .unwrap();
        let context = egui::Context::default();
        let mut viewer = Viewer::new(context.clone());
        render(&mut viewer, &context, Vec::new());
        viewer.open_folder(folder.path().to_owned(), &context);
        wait_until(&mut viewer, &context, |viewer| {
            viewer.preview.is_some() && viewer.thumbnails.len() == 2
        });
        assert_eq!(viewer.selected, Some(0));
        assert_eq!(viewer.metadata.as_ref().unwrap().width, 40);
        viewer.grid_view = false;
        let output = render(&mut viewer, &context, Vec::new());
        let text_position = |label: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == label => Some(text.pos),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing UI label: {label}"))
        };
        let heading = text_position("IMAGE DETAILS");
        let dimensions = text_position("40 x 20 px");
        assert!(heading.x > 1000.0 && heading.y > 50.0);
        assert!(
            heading.y < dimensions.y,
            "Properties must read from top to bottom"
        );
        render(
            &mut viewer,
            &context,
            vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(viewer.selected, Some(1));
        wait_until(&mut viewer, &context, |viewer| viewer.preview.is_some());
        assert_eq!(viewer.metadata.as_ref().unwrap().width, 60);
        viewer.open_folder(folder.path().to_owned(), &context);
        viewer.open_folder(other.path().to_owned(), &context);
        wait_until(&mut viewer, &context, |viewer| viewer.preview.is_some());
        assert_eq!(viewer.paths, vec![other.path().join("new.png")]);
        let metadata = viewer.metadata.as_ref().unwrap();
        assert_eq!((metadata.width, metadata.height), (12, 24));
    }

    #[test]
    fn ui_shows_empty_folder_and_corrupt_image_without_stalling() {
        let folder = tempfile::tempdir().unwrap();
        let context = egui::Context::default();
        let mut viewer = Viewer::new(context.clone());
        viewer.open_folder(folder.path().to_owned(), &context);
        wait_until(&mut viewer, &context, |viewer| !viewer.scanning);
        assert!(viewer.paths.is_empty());
        assert!(viewer.selected.is_none());
        std::fs::write(folder.path().join("broken.png"), b"broken image").unwrap();
        viewer.open_folder(folder.path().to_owned(), &context);
        wait_until(&mut viewer, &context, |viewer| {
            viewer.preview_error.is_some() && !viewer.errors.is_empty()
        });
        assert!(viewer.preview.is_none());
        viewer.open_folder(folder.path().join("missing"), &context);
        wait_until(&mut viewer, &context, |viewer| {
            viewer.folder_error.is_some()
        });
        assert!(!viewer.scanning);
    }

    #[test]
    fn large_folder_only_loads_visible_thumbnails_and_reveals_keyboard_selection() {
        let folder = tempfile::tempdir().unwrap();
        let first = folder.path().join("000.png");
        image::RgbaImage::new(8, 4).save(&first).unwrap();
        for index in 1..300 {
            std::fs::copy(&first, folder.path().join(format!("{index:03}.png"))).unwrap();
        }
        let context = egui::Context::default();
        let mut viewer = Viewer::new(context.clone());
        viewer.open_folder(folder.path().to_owned(), &context);
        wait_until(&mut viewer, &context, |viewer| {
            viewer.preview.is_some() && !viewer.thumbnails.is_empty()
        });
        assert_eq!(viewer.paths.len(), 300);
        assert!(viewer.thumbnails.len() < 100);
        assert!(viewer.pending.len() <= MAX_PENDING);
        render(
            &mut viewer,
            &context,
            vec![egui::Event::Key {
                key: egui::Key::ArrowLeft,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(viewer.selected, Some(299));
        wait_until(&mut viewer, &context, |viewer| {
            viewer.preview.is_some() && viewer.thumbnails.contains_key(&299)
        });
        assert!(viewer.thumbnails.len() < 160);
    }
}
