use crate::config::{DoubleClickAction, FilmstripVisibility, FilterMode, ViewerConfig, WindowMode};
use egui::{Context, Window};

#[derive(Default)]
pub struct SettingsDialog {
    pub is_open: bool,
}

impl SettingsDialog {
    pub fn show(&mut self, ctx: &Context, config: &mut ViewerConfig) -> bool {
        if !self.is_open {
            return false;
        }

        let mut config_changed = false;
        let mut open = self.is_open;

        Window::new("Настройки")
            .open(&mut open)
            .resizable(true)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::new(8.0, 10.0);

                // --- 1. WINDOW SECTION ---
                ui.heading("Window & Display");
                ui.horizontal(|ui| {
                    ui.label("Default Mode:");
                    if ui
                        .selectable_label(
                            config.window.mode == WindowMode::Overlay,
                            "Fullscreen overlay",
                        )
                        .clicked()
                    {
                        config.window.mode = WindowMode::Overlay;
                        config_changed = true;
                    }
                    if ui
                        .selectable_label(config.window.mode == WindowMode::Windowed, "Windowed")
                        .clicked()
                    {
                        config.window.mode = WindowMode::Windowed;
                        config_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Double-click canvas:");
                    egui::ComboBox::from_id_salt("double_click_action")
                        .selected_text(match config.window.double_click_action {
                            DoubleClickAction::ToggleWindowMode => "Toggle Window / Overlay Mode",
                            DoubleClickAction::ToggleFitToWindow => "Toggle Fit to Window",
                            DoubleClickAction::ToggleActualSize => "Toggle 1:1 Actual Size",
                            DoubleClickAction::None => "None",
                        })
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(
                                    &mut config.window.double_click_action,
                                    DoubleClickAction::ToggleWindowMode,
                                    "Toggle Window / Overlay Mode",
                                )
                                .clicked()
                            {
                                config_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut config.window.double_click_action,
                                    DoubleClickAction::ToggleFitToWindow,
                                    "Toggle Fit to Window",
                                )
                                .clicked()
                            {
                                config_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut config.window.double_click_action,
                                    DoubleClickAction::ToggleActualSize,
                                    "Toggle 1:1 Actual Size",
                                )
                                .clicked()
                            {
                                config_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut config.window.double_click_action,
                                    DoubleClickAction::None,
                                    "None",
                                )
                                .clicked()
                            {
                                config_changed = true;
                            }
                        });
                });

                if ui
                    .checkbox(
                        &mut config.window.transparent_windowed_background,
                        "Transparent background in windowed mode",
                    )
                    .changed()
                {
                    config_changed = true;
                }

                if ui
                    .checkbox(
                        &mut config.window.show_checkerboard_for_transparent,
                        "Show checkerboard background for transparent PNG/WebP",
                    )
                    .changed()
                {
                    config_changed = true;
                }

                if ui
                    .checkbox(
                        &mut config.window.auto_hide_ui,
                        "Auto-hide HUD and navigation chevrons on idle",
                    )
                    .changed()
                {
                    config_changed = true;
                }

                ui.separator();

                // --- 2. RENDERING & ZOOM SECTION ---
                ui.heading("Rendering & Zoom");
                ui.horizontal(|ui| {
                    ui.label("Texture Filtering:");
                    if ui
                        .selectable_label(
                            config.rendering.filter_mode == FilterMode::Auto,
                            "Auto (Crisp zoom)",
                        )
                        .clicked()
                    {
                        config.rendering.filter_mode = FilterMode::Auto;
                        config_changed = true;
                    }
                    if ui
                        .selectable_label(
                            config.rendering.filter_mode == FilterMode::Nearest,
                            "Nearest (Pixel art)",
                        )
                        .clicked()
                    {
                        config.rendering.filter_mode = FilterMode::Nearest;
                        config_changed = true;
                    }
                    if ui
                        .selectable_label(
                            config.rendering.filter_mode == FilterMode::Bilinear,
                            "Bilinear (Smooth)",
                        )
                        .clicked()
                    {
                        config.rendering.filter_mode = FilterMode::Bilinear;
                        config_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Zoom speed factor:");
                    if ui
                        .add(
                            egui::Slider::new(&mut config.rendering.zoom_factor, 1.05..=1.50)
                                .step_by(0.01),
                        )
                        .changed()
                    {
                        config_changed = true;
                    }
                });

                if ui
                    .checkbox(
                        &mut config.rendering.zoom_towards_cursor,
                        "Zoom centered towards mouse cursor",
                    )
                    .changed()
                {
                    config_changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.rendering.reverse_zoom_direction,
                        "Invert mouse wheel zoom direction",
                    )
                    .changed()
                {
                    config_changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.rendering.smooth_pan,
                        "Короткая инерция перемещения",
                    )
                    .changed()
                {
                    config_changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.rendering.animate_image_transitions,
                        "Анимация перелистывания изображений",
                    )
                    .changed()
                {
                    config_changed = true;
                }
                ui.add_enabled_ui(config.rendering.animate_image_transitions, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Длительность перехода:");
                        if ui
                            .add(
                                egui::Slider::new(
                                    &mut config.rendering.image_transition_ms,
                                    80.0..=500.0,
                                )
                                .suffix(" мс"),
                            )
                            .changed()
                        {
                            config_changed = true;
                        }
                    });
                });

                ui.separator();

                // --- 3. NAVIGATION SECTION ---
                ui.heading("Navigation & Controls");
                if ui
                    .checkbox(
                        &mut config.navigation.loop_folder,
                        "Wrap around folder (loop last to first)",
                    )
                    .changed()
                {
                    config_changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.navigation.edge_buttons_enabled,
                        "Enable interactive screen edge chevrons (< and >)",
                    )
                    .changed()
                {
                    config_changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.navigation.animate_buttons,
                        "Анимация кнопок перелистывания",
                    )
                    .changed()
                {
                    config_changed = true;
                }

                ui.separator();

                // --- 4. FILMSTRIP (BOTTOM GALLERY) SECTION ---
                ui.heading("Bottom Filmstrip Gallery");
                ui.horizontal(|ui| {
                    ui.label("Visibility:");
                    if ui
                        .selectable_label(
                            config.filmstrip.visibility == FilmstripVisibility::Hover,
                            "Hover (Auto-show)",
                        )
                        .clicked()
                    {
                        config.filmstrip.visibility = FilmstripVisibility::Hover;
                        config_changed = true;
                    }
                    if ui
                        .selectable_label(
                            config.filmstrip.visibility == FilmstripVisibility::Always,
                            "Always Visible",
                        )
                        .clicked()
                    {
                        config.filmstrip.visibility = FilmstripVisibility::Always;
                        config_changed = true;
                    }
                    if ui
                        .selectable_label(
                            config.filmstrip.visibility == FilmstripVisibility::Hidden,
                            "Hidden",
                        )
                        .clicked()
                    {
                        config.filmstrip.visibility = FilmstripVisibility::Hidden;
                        config_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Thumbnail size (px):");
                    if ui
                        .add(egui::Slider::new(
                            &mut config.filmstrip.thumbnail_size,
                            48..=128,
                        ))
                        .changed()
                    {
                        config_changed = true;
                    }
                });

                if ui
                    .checkbox(
                        &mut config.filmstrip.auto_scroll_to_active,
                        "Auto-scroll filmstrip to active image",
                    )
                    .changed()
                {
                    config_changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.filmstrip.coverflow_effect,
                        "Увеличивать превью ближе к центру",
                    )
                    .changed()
                {
                    config_changed = true;
                }

                ui.separator();

                // --- 5. CACHE & PERFORMANCE SECTION ---
                ui.heading("Cache & Preloading");
                ui.horizontal(|ui| {
                    ui.label("RAM Cache Limit (MB):");
                    if ui
                        .add(
                            egui::Slider::new(&mut config.cache.max_ram_cache_mb, 256..=4096)
                                .step_by(128.0),
                        )
                        .changed()
                    {
                        config_changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Prefetch adjacent images:");
                    if ui
                        .add(egui::Slider::new(&mut config.cache.prefetch_count, 0..=6))
                        .changed()
                    {
                        config_changed = true;
                    }
                });

                ui.separator();

                // Footer buttons
                ui.horizontal(|ui| {
                    if ui.button("Сохранить").clicked() {
                        let _ = config.save();
                        self.is_open = false;
                    }

                    if ui.button("Сбросить").clicked() {
                        *config = ViewerConfig::default();
                        let _ = config.save();
                        config_changed = true;
                    }

                    if ui.button("Закрыть").clicked() {
                        self.is_open = false;
                    }
                });
            });

        self.is_open = open;
        config_changed
    }
}
