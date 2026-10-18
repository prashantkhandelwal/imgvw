use super::theme::{BACKGROUND, BUTTON_BACKGROUND, MENU_FONT_SIZE};
use super::*;
use crate::settings;
use eframe::egui::{Color32, Vec2};

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
fn filename_hover_captions_are_shown_on_thumbnails_but_not_preview_images() {
    fn visible_image(output: &egui::FullOutput, texture: egui::TextureId) -> egui::Rect {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                    Some(mesh.calc_bounds().intersect(shape.clip_rect))
                }
                _ => None,
            })
            .expect("Image should be visible")
    }

    fn caption(
        output: &egui::FullOutput,
        name: &str,
        image: egui::Rect,
    ) -> Option<(egui::Pos2, Vec2, usize)> {
        output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == name && image.contains(text.pos) => {
                Some((text.pos, text.galley.size(), text.galley.rows.len()))
            }
            _ => None,
        })
    }

    for name in [
        "photo.png".to_string(),
        format!("{}.png", "long-name-".repeat(14)),
    ] {
        let folder = tempfile::tempdir().unwrap();
        image::RgbaImage::new(1280, 960)
            .save(folder.path().join(&name))
            .unwrap();
        let context = egui::Context::default();
        context.style_mut(|style| style.animation_time = 0.0);
        let mut viewer = Viewer::new(context.clone());
        viewer.open_folder(folder.path().to_owned(), &context);
        wait_until(&mut viewer, &context, |viewer| {
            viewer.preview.is_some() && viewer.thumbnails.contains_key(&0)
        });
        for (grid_view, zoom) in [(true, 100.0), (false, 100.0), (false, 400.0)] {
            viewer.grid_view = grid_view;
            viewer.preview_zoom = zoom;
            render(
                &mut viewer,
                &context,
                vec![egui::Event::PointerMoved(egui::pos2(5.0, 5.0))],
            );
            let output = render(&mut viewer, &context, Vec::new());
            let texture = if grid_view {
                viewer.thumbnails[&0].texture.id()
            } else {
                viewer.preview.as_ref().unwrap().id()
            };
            let visible = visible_image(&output, texture);
            assert!(
                caption(&output, &name, visible).is_none(),
                "Caption must be hidden before hover"
            );
            let output = render(
                &mut viewer,
                &context,
                vec![egui::Event::PointerMoved(visible.center())],
            );
            let visible = visible_image(&output, texture);
            if grid_view {
                let (position, size, rows) = caption(&output, &name, visible)
                    .expect("Hovered thumbnail should show its filename");
                assert!(
                    position.y >= visible.bottom() - 30.0,
                    "Filename should be at the visible bottom"
                );
                assert_eq!(rows, 1, "Long filenames should stay on one line");
                assert!(
                    size.x <= visible.width() - 12.0 + 1.0,
                    "Caption should fit within the image"
                );
            } else {
                assert!(
                    caption(&output, &name, visible).is_none(),
                    "The preview image must not show a filename hover caption"
                );
            }
            let output = render(
                &mut viewer,
                &context,
                vec![egui::Event::PointerMoved(egui::pos2(5.0, 5.0))],
            );
            assert!(
                caption(&output, &name, visible).is_none(),
                "Caption must disappear after leaving"
            );
        }
    }
}

#[test]
fn folder_heading_shows_the_total_image_count_in_both_views() {
    for grid_view in [true, false] {
        for count in [0, 1, 24] {
            let context = egui::Context::default();
            let mut viewer = Viewer::new(context.clone());
            viewer.grid_view = grid_view;
            viewer.folder = Some(PathBuf::from("example-folder"));
            viewer.paths = (0..count)
                .map(|index| PathBuf::from(format!("{index}.png")))
                .collect();
            render(&mut viewer, &context, Vec::new());
            let output = render(&mut viewer, &context, Vec::new());
            let folder = text_position(&output, "example-folder");
            let label = format!("{count} {}", if count == 1 { "image" } else { "images" });
            let total = text_position(&output, &label);
            assert!(total.x > folder.x);
            assert!((total.y - folder.y).abs() < 2.0);
            assert!(total.x < 1280.0);
        }
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
fn preferences_save_and_ok_persist_edits_and_restore_startup_defaults() {
    let directory = tempfile::tempdir().unwrap();
    let photos = directory.path().join("photos");
    std::fs::create_dir(&photos).unwrap();
    image::RgbaImage::new(40, 20)
        .save(photos.join("photo.png"))
        .unwrap();
    let path = directory.path().join("config").join("settings.json");
    let context = egui::Context::default();
    let mut viewer = Viewer::new(context.clone());
    viewer.settings_path = Some(path.clone());
    viewer.preferences_open = true;
    viewer.preferences_draft = Settings {
        default_folder: photos.to_str().unwrap().into(),
        default_view: DefaultView::Preview,
    };
    render(&mut viewer, &context, Vec::new());
    let output = render(&mut viewer, &context, Vec::new());
    let save = text_position(&output, "Save");
    let ok = text_position(&output, "Ok");
    let folder_label = text_position(&output, "Default folder location");
    let view_label = text_position(&output, "Default view");
    assert!(save.x < ok.x && save.x > folder_label.x);
    assert!(save.y > view_label.y);
    click(&mut viewer, &context, save);
    assert!(viewer.preferences_open, "Save must leave Preferences open");
    assert_eq!(settings::load(&path).unwrap(), viewer.preferences_draft);
    assert!(!viewer.grid_view);
    assert!(viewer.preferences_error.is_none());

    let mut restarted = Viewer::new(context.clone());
    restarted.load_settings(&path, &context);
    assert_eq!(restarted.folder.as_deref(), Some(photos.as_path()));
    assert!(!restarted.grid_view);
    wait_until(&mut restarted, &context, |viewer| viewer.preview.is_some());

    viewer.preferences_draft.default_view = DefaultView::Grid;
    render(&mut viewer, &context, Vec::new());
    let output = render(&mut viewer, &context, Vec::new());
    click(&mut viewer, &context, text_position(&output, "Ok"));
    assert!(
        !viewer.preferences_open,
        "Ok must save and close Preferences"
    );
    assert_eq!(
        settings::load(&path).unwrap().default_view,
        DefaultView::Grid
    );
    assert!(viewer.grid_view);
    restarted.load_settings(&path, &context);
    assert!(restarted.grid_view);
}

#[test]
fn preferences_reports_errors_without_closing_or_losing_saved_settings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    settings::save(&path, &Settings::default()).unwrap();
    let context = egui::Context::default();
    let mut viewer = Viewer::new(context.clone());
    viewer.load_settings(&path, &context);
    viewer.preferences_open = true;
    viewer.preferences_draft.default_folder =
        directory.path().join("missing").display().to_string();
    render(&mut viewer, &context, Vec::new());
    let output = render(&mut viewer, &context, Vec::new());
    click(&mut viewer, &context, text_position(&output, "Ok"));
    assert!(viewer.preferences_open);
    assert!(
        viewer
            .preferences_error
            .as_ref()
            .unwrap()
            .contains("existing folder")
    );
    assert_eq!(settings::load(&path).unwrap(), Settings::default());
    assert_eq!(viewer.settings, Settings::default());
    let output = render(&mut viewer, &context, Vec::new());
    text_position(&output, viewer.preferences_error.as_ref().unwrap());

    viewer.preferences_draft = Settings::default();
    viewer.settings_path = Some(directory.path().to_owned());
    assert!(!viewer.save_preferences());
    assert!(
        viewer
            .preferences_error
            .as_ref()
            .unwrap()
            .contains("Cannot save settings")
    );
    std::fs::write(&path, b"broken settings").unwrap();
    let mut restarted = Viewer::new(context.clone());
    restarted.load_settings(&path, &context);
    assert!(restarted.preferences_open);
    assert!(
        restarted
            .preferences_error
            .as_ref()
            .unwrap()
            .contains("Invalid settings")
    );
}

#[test]
fn preferences_folder_picker_updates_only_the_draft_and_handles_cancellation() {
    let directory = tempfile::tempdir().unwrap();
    let context = egui::Context::default();
    let mut viewer = Viewer::new(context.clone());
    viewer.preferences_open = true;
    let (sender, receiver) = mpsc::channel();
    viewer.preferences_picker = Some(receiver);
    sender.send(Some(directory.path().to_owned())).unwrap();
    viewer.receive(&context);
    assert_eq!(
        viewer.preferences_draft.default_folder,
        directory.path().to_str().unwrap()
    );
    assert!(viewer.settings.default_folder.is_empty());
    assert!(viewer.folder.is_none());
    assert!(viewer.preferences_picker.is_none());

    let (sender, receiver) = mpsc::channel();
    viewer.preferences_picker = Some(receiver);
    sender.send(None).unwrap();
    viewer.receive(&context);
    assert_eq!(
        viewer.preferences_draft.default_folder,
        directory.path().to_str().unwrap()
    );
    assert!(viewer.preferences_error.is_none());

    let (sender, receiver) = mpsc::channel();
    viewer.preferences_picker = Some(receiver);
    drop(sender);
    viewer.receive(&context);
    assert!(
        viewer
            .preferences_error
            .as_ref()
            .unwrap()
            .contains("unexpectedly")
    );
}

#[test]
fn file_menu_opens_preferences_and_requests_exit() {
    let context = egui::Context::default();
    let mut viewer = Viewer::new(context.clone());
    render(&mut viewer, &context, Vec::new());
    let output = render(&mut viewer, &context, Vec::new());
    let file_position = text_position(&output, "File");
    let menu_font = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == "File" => {
                Some(text.galley.job.sections[0].format.font_id.clone())
            }
            _ => None,
        })
        .expect("File menu font should be available");
    click(&mut viewer, &context, file_position);
    let output = render(&mut viewer, &context, Vec::new());
    for label in ["Open Folder(s)", "Preferences", "Exit"] {
        assert!(
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text)
                if text.galley.job.text == label
                && text.galley.job.sections.iter().all(|section| {
                    section.format.font_id == menu_font
                }))
            }),
            "{label} must use the menu bar font"
        );
    }
    text_position(&output, "Open Folder(s)");
    text_position(&output, "Exit");
    click(&mut viewer, &context, text_position(&output, "Preferences"));
    assert!(viewer.preferences_open);
    let output = render(&mut viewer, &context, Vec::new());
    text_position(&output, "Default folder location");

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
fn buttons_contrast_with_panels_and_have_distinct_interaction_states() {
    let context = egui::Context::default();
    context.style_mut(|style| style.animation_time = 0.0);
    let mut viewer = Viewer::new(context.clone());
    let style = context.style();
    assert_ne!(BUTTON_BACKGROUND, BACKGROUND);
    assert_eq!(
        style.visuals.widgets.inactive.weak_bg_fill,
        BUTTON_BACKGROUND
    );
    assert_ne!(
        style.visuals.widgets.hovered.weak_bg_fill,
        BUTTON_BACKGROUND
    );
    assert_ne!(
        style.visuals.widgets.active.weak_bg_fill,
        style.visuals.widgets.hovered.weak_bg_fill,
    );
    viewer.grid_view = false;
    viewer.preferences_open = true;
    render(&mut viewer, &context, Vec::new());
    let output = render(&mut viewer, &context, Vec::new());
    let file_position = text_position(&output, "File");
    assert!(
        !output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Rect(rect)
            if rect.rect.contains(file_position) && rect.fill == BUTTON_BACKGROUND)
        }),
        "The menu bar should not use the contrasting button fill"
    );
    for label in ["Photos", "Browse...", "Save", "Ok", "Choose a folder"] {
        let position = text_position(&output, label);
        assert!(
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect)
                if rect.rect.contains(position) && rect.fill == BUTTON_BACKGROUND)
            }),
            "{label} should have a contrasting button fill"
        );
    }
    viewer.preferences_open = false;
    render(&mut viewer, &context, Vec::new());
    click(&mut viewer, &context, text_position(&output, "File"));
    let output = render(&mut viewer, &context, Vec::new());
    for label in ["Open Folder(s)", "Preferences", "Exit"] {
        let position = text_position(&output, label);
        assert!(
            !output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect)
                if rect.rect.contains(position) && rect.fill == BUTTON_BACKGROUND)
            }),
            "{label} should retain the menu background"
        );
    }
}

#[test]
fn menus_preferences_and_inputs_use_the_shared_background() {
    let context = egui::Context::default();
    let mut viewer = Viewer::new(context.clone());
    let style = context.style();
    for color in [
        egui::Frame::menu(&style).fill,
        egui::Frame::window(&style).fill,
        style.visuals.extreme_bg_color,
        style.visuals.faint_bg_color,
        style.visuals.code_bg_color,
        style.visuals.widgets.noninteractive.bg_fill,
    ] {
        assert_eq!(color, BACKGROUND);
    }
    render(&mut viewer, &context, Vec::new());
    let output = render(&mut viewer, &context, Vec::new());
    click(&mut viewer, &context, text_position(&output, "File"));
    let output = render(&mut viewer, &context, Vec::new());
    let menu_item = text_position(&output, "Open Folder(s)");
    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(rect)
            if rect.rect.contains(menu_item) && rect.fill == BACKGROUND)
    }));
    click(&mut viewer, &context, text_position(&output, "Preferences"));
    render(&mut viewer, &context, Vec::new());
    let output = render(&mut viewer, &context, Vec::new());
    let field_label = text_position(&output, "Default folder location");
    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(rect)
            if rect.rect.contains(field_label) && rect.fill == BACKGROUND)
    }));
}

#[test]
fn preview_details_pane_matches_menu_bar_color() {
    let context = egui::Context::default();
    let mut viewer = Viewer::new(context.clone());
    viewer.grid_view = false;
    render(&mut viewer, &context, Vec::new());
    let output = render(&mut viewer, &context, Vec::new());
    for (point, color) in [
        (egui::pos2(5.0, 5.0), BACKGROUND),
        (egui::pos2(1270.0, 170.0), BACKGROUND),
        (egui::pos2(1270.0, 600.0), BACKGROUND),
        (egui::pos2(5.0, 170.0), BACKGROUND),
    ] {
        assert!(
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect)
                    if rect.rect.contains(point) && rect.fill == color)
            }),
            "Unexpected preview panel color at {point:?}"
        );
    }
}

#[test]
fn grid_uses_the_shared_background_everywhere() {
    assert_eq!(BACKGROUND, Color32::from_rgb(43, 43, 43));
    let context = egui::Context::default();
    let mut viewer = Viewer::new(context.clone());
    render(&mut viewer, &context, Vec::new());
    let output = render(&mut viewer, &context, Vec::new());
    assert_eq!(context.style().visuals.panel_fill, BACKGROUND);
    assert_eq!(context.style().visuals.window_fill, BACKGROUND);
    for (point, color) in [
        (egui::pos2(5.0, 5.0), BACKGROUND),
        (egui::pos2(5.0, 810.0), BACKGROUND),
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
fn preview_zoom_slider_resizes_and_pans_without_changing_thumbnails() {
    fn preview_rect(
        output: &egui::FullOutput,
        texture: egui::TextureId,
    ) -> (egui::Rect, egui::Rect) {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                    Some((mesh.calc_bounds(), shape.clip_rect))
                }
                _ => None,
            })
            .expect("Preview image should be rendered")
    }

    let folder = tempfile::tempdir().unwrap();
    image::RgbaImage::new(1600, 1200)
        .save(folder.path().join("photo.png"))
        .unwrap();
    let context = egui::Context::default();
    context.style_mut(|style| style.animation_time = 0.0);
    let mut viewer = Viewer::new(context.clone());
    viewer.open_folder(folder.path().to_owned(), &context);
    wait_until(&mut viewer, &context, |viewer| viewer.preview.is_some());
    viewer.grid_view = false;
    render(&mut viewer, &context, Vec::new());
    let output = render(&mut viewer, &context, Vec::new());
    text_position(&output, "Zoom");
    let texture = viewer.preview.as_ref().unwrap().id();
    let initial = preview_rect(&output, texture).0;
    let slider_position = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle) if circle.center.y > 780.0 => {
                Some(circle.center + Vec2::new(20.0, 0.0))
            }
            _ => None,
        })
        .expect("Zoom slider handle should be visible");
    click(&mut viewer, &context, slider_position);
    assert!(viewer.preview_zoom > 100.0);
    assert_eq!(viewer.thumbnail_size, 144.0);
    let output = render(&mut viewer, &context, Vec::new());
    let enlarged = preview_rect(&output, texture).0;
    assert!((enlarged.width() / initial.width() - viewer.preview_zoom / 100.0).abs() < 0.01);

    for percent in [25.0, 100.0, 400.0] {
        viewer.preview_zoom = percent;
        render(&mut viewer, &context, Vec::new());
        let output = render(&mut viewer, &context, Vec::new());
        let (image, clip) = preview_rect(&output, texture);
        assert!((image.width() / initial.width() - percent / 100.0).abs() < 0.01);
        assert!((image.height() / initial.height() - percent / 100.0).abs() < 0.01);
        if percent == 400.0 {
            assert!(image.width() > clip.width() && image.height() > clip.height());
            render(
                &mut viewer,
                &context,
                vec![
                    egui::Event::PointerMoved(clip.center()),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: Vec2::new(0.0, -120.0),
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            let output = render(&mut viewer, &context, Vec::new());
            let before_drag = preview_rect(&output, texture).0;
            assert!(before_drag.min.y < image.min.y);
            assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::Grab);
            let start = clip.center();
            render(
                &mut viewer,
                &context,
                vec![egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            let end = start - Vec2::new(80.0, 60.0);
            let output = render(&mut viewer, &context, vec![egui::Event::PointerMoved(end)]);
            assert_eq!(
                output.platform_output.cursor_icon,
                egui::CursorIcon::Grabbing
            );
            let output = render(&mut viewer, &context, Vec::new());
            let after_drag = preview_rect(&output, texture).0;
            assert!(
                after_drag.min.x < before_drag.min.x,
                "Dragging left must pan horizontally"
            );
            assert!(
                after_drag.min.y < before_drag.min.y,
                "Dragging up must pan vertically"
            );
            let output = render(
                &mut viewer,
                &context,
                vec![egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::Grab);
        } else {
            let output = render(
                &mut viewer,
                &context,
                vec![egui::Event::PointerMoved(image.center())],
            );
            assert_ne!(output.platform_output.cursor_icon, egui::CursorIcon::Grab);
        }
    }
    viewer.grid_view = true;
    let output = render(&mut viewer, &context, Vec::new());
    text_position(&output, "Thumbnail size");
    assert_eq!(viewer.thumbnail_size, 144.0);
    viewer.grid_view = false;
    render(&mut viewer, &context, Vec::new());
    assert_eq!(viewer.preview_zoom, 400.0);
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
fn system_theme_cannot_override_the_shared_background() {
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
        assert_eq!(context.style().visuals.panel_fill, BACKGROUND);
        assert_eq!(context.style().visuals.window_fill, BACKGROUND);
        for (point, color) in [
            (egui::pos2(5.0, 5.0), BACKGROUND),
            (egui::pos2(5.0, 810.0), BACKGROUND),
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
fn recursive_scan_progress_is_displayed_and_ignores_stale_updates() {
    let context = egui::Context::default();
    let mut viewer = Viewer::new(context.clone());
    let (sender, receiver) = mpsc::channel();
    viewer.loader.events = receiver;
    viewer.generation = 2;
    viewer.scanning = true;
    sender
        .send(Event::ScanProgress {
            generation: 1,
            progress: ScanProgress {
                files_read: 999,
                images_found: 999,
            },
        })
        .unwrap();
    sender
        .send(Event::ScanProgress {
            generation: 2,
            progress: ScanProgress {
                files_read: 42,
                images_found: 12,
            },
        })
        .unwrap();
    for grid_view in [true, false] {
        viewer.grid_view = grid_view;
        let output = render(&mut viewer, &context, Vec::new());
        text_position(
            &output,
            "Reading recursively: 42 files checked, 12 images found",
        );
        assert_eq!(
            viewer.scan_progress,
            ScanProgress {
                files_read: 42,
                images_found: 12
            }
        );
    }
    sender
        .send(Event::Folder {
            generation: 2,
            path: PathBuf::from("finished"),
            result: Ok(Vec::new()),
        })
        .unwrap();
    sender
        .send(Event::ScanProgress {
            generation: 2,
            progress: ScanProgress {
                files_read: 100,
                images_found: 100,
            },
        })
        .unwrap();
    render(&mut viewer, &context, Vec::new());
    assert!(!viewer.scanning);
    assert_eq!(viewer.scan_progress.files_read, 42);
    let folder = tempfile::tempdir().unwrap();
    viewer.open_folder(folder.path().to_owned(), &context);
    assert_eq!(viewer.scan_progress, ScanProgress::default());
}

#[test]
fn opening_a_folder_loads_nested_images_and_counts_them_in_both_views() {
    let folder = tempfile::tempdir().unwrap();
    let nested = folder.path().join("nested");
    let deep = nested.join("deeper");
    std::fs::create_dir_all(&deep).unwrap();
    let paths = vec![
        folder.path().join("a.png"),
        nested.join("b.png"),
        deep.join("c.png"),
    ];
    for (index, path) in paths.iter().enumerate() {
        image::RgbaImage::new(40 + index as u32 * 20, 20)
            .save(path)
            .unwrap();
    }
    let context = egui::Context::default();
    let mut viewer = Viewer::new(context.clone());
    viewer.open_folder(folder.path().to_owned(), &context);
    wait_until(&mut viewer, &context, |viewer| {
        viewer.preview.is_some() && viewer.thumbnails.len() == 3
    });
    assert_eq!(viewer.paths, paths);
    assert_eq!(
        viewer.scan_progress,
        ScanProgress {
            files_read: 3,
            images_found: 3
        }
    );
    assert_eq!(viewer.folder.as_deref(), Some(folder.path()));
    for grid_view in [true, false] {
        viewer.grid_view = grid_view;
        let output = render(&mut viewer, &context, Vec::new());
        text_position(&output, "3 images");
    }
    viewer.select(2);
    wait_until(&mut viewer, &context, |viewer| viewer.preview.is_some());
    assert_eq!(viewer.metadata.as_ref().unwrap().width, 80);
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
