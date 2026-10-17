use eframe::egui::{self, Color32};

pub(super) const ACCENT: Color32 = Color32::from_rgb(239, 171, 86);
pub(super) const BACKGROUND: Color32 = Color32::from_rgb(43, 43, 43);
pub(super) const BUTTON_BACKGROUND: Color32 = Color32::from_rgb(75, 75, 75);
pub(super) const MENU_FONT_SIZE: f32 = 16.0;

pub(super) fn apply(context: &egui::Context) {
    context.set_theme(egui::Theme::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = BACKGROUND;
    visuals.extreme_bg_color = BACKGROUND;
    visuals.faint_bg_color = BACKGROUND;
    visuals.code_bg_color = BACKGROUND;
    visuals.widgets.noninteractive.bg_fill = BACKGROUND;
    visuals.widgets.noninteractive.weak_bg_fill = BACKGROUND;
    for (state, color) in [
        (&mut visuals.widgets.inactive, BUTTON_BACKGROUND),
        (&mut visuals.widgets.hovered, Color32::from_gray(90)),
        (&mut visuals.widgets.active, Color32::from_gray(105)),
        (&mut visuals.widgets.open, Color32::from_gray(90)),
    ] {
        state.bg_fill = color;
        state.weak_bg_fill = color;
    }
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.3);
    visuals.selection.stroke.color = ACCENT;
    context.set_visuals(visuals);
}
