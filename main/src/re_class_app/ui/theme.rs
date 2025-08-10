use eframe::egui::{
    self,
    Color32,
    Context,
    FontDefinitions,
    FontFamily,
    FontId,
    TextStyle,
    Visuals,
};

use super::ReClassGui;

impl ReClassGui {
    pub(super) fn apply_theme_once(&mut self, ctx: &Context) {
        if self.theme_applied {
            return;
        }

        // Fonts
        let fonts = FontDefinitions::default();
        ctx.set_fonts(fonts);

        // Style
        let mut style = (*ctx.style()).clone();

        let mut visuals = Visuals::dark();
        visuals.dark_mode = true;
        visuals.window_rounding = 8.0.into();
        visuals.window_shadow.offset = egui::vec2(0.0, 2.0);
        visuals.window_shadow.blur = 12.0;
        visuals.window_shadow.spread = 0.0;
        visuals.window_shadow.color = Color32::from_black_alpha(80);
        visuals.panel_fill = Color32::from_rgb(20, 22, 28);
        visuals.extreme_bg_color = Color32::from_rgb(16, 18, 24);
        visuals.faint_bg_color = Color32::from_rgb(30, 33, 40);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(35, 39, 48);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 50, 62);
        visuals.widgets.active.bg_fill = Color32::from_rgb(55, 60, 74);
        visuals.selection.bg_fill = Color32::from_rgb(60, 110, 200);
        visuals.hyperlink_color = Color32::from_rgb(120, 170, 255);
        visuals.widgets.inactive.rounding = 6.0.into();
        visuals.widgets.hovered.rounding = 6.0.into();
        visuals.widgets.active.rounding = 6.0.into();
        visuals.widgets.open.rounding = 6.0.into();
        visuals.widgets.noninteractive.bg_fill = visuals.panel_fill;
        style.visuals = visuals;

        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 5.0);
        style.spacing.window_margin = egui::Margin::symmetric(12.0, 12.0);
        style.spacing.interact_size.y = 24.0;

        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(20.0, FontFamily::Proportional),
        );
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(16.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(15.0, FontFamily::Monospace),
        );
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(15.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(13.0, FontFamily::Proportional),
        );

        ctx.set_style(style);
        self.theme_applied = true;
    }
}
