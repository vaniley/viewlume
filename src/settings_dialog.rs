use crate::config::{
    DoubleClickAction, FilmstripVisibility, FilterMode, Language, ViewerConfig, WindowMode,
};
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
        let mut should_close = false;
        let ru = config.general.language.is_russian();
        let tr = |english: &'static str, russian: &'static str| if ru { russian } else { english };

        Window::new(tr("Settings", "Настройки"))
            .open(&mut open)
            .resizable(true)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::new(8.0, 10.0);

                ui.heading(tr("Language", "Язык"));
                ui.horizontal(|ui| {
                    ui.label(tr("Interface language:", "Язык интерфейса:"));
                    for (language, label) in [
                        (Language::Auto, tr("System", "Системный")),
                        (Language::English, "English"),
                        (Language::Russian, "Русский"),
                    ] {
                        if ui
                            .selectable_value(&mut config.general.language, language, label)
                            .clicked()
                        {
                            config_changed = true;
                        }
                    }
                });
                ui.separator();

                // --- 1. WINDOW SECTION ---
                ui.heading(tr("Window & Display", "Окно и экран"));
                ui.horizontal(|ui| {
                    ui.label(tr("Default mode:", "Режим по умолчанию:"));
                    if ui
                        .selectable_label(
                            config.window.mode == WindowMode::Overlay,
                            tr("Fullscreen overlay", "Полноэкранный оверлей"),
                        )
                        .clicked()
                    {
                        config.window.mode = WindowMode::Overlay;
                        config_changed = true;
                    }
                    if ui
                        .selectable_label(
                            config.window.mode == WindowMode::Windowed,
                            tr("Windowed", "Оконный"),
                        )
                        .clicked()
                    {
                        config.window.mode = WindowMode::Windowed;
                        config_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(tr("Double-click canvas:", "Двойной клик:"));
                    egui::ComboBox::from_id_salt("double_click_action")
                        .selected_text(match config.window.double_click_action {
                            DoubleClickAction::ToggleWindowMode => {
                                tr("Toggle window mode", "Переключать режим окна")
                            }
                            DoubleClickAction::ToggleFitToWindow => {
                                tr("Toggle fit to window", "Вписывать в окно")
                            }
                            DoubleClickAction::ToggleActualSize => {
                                tr("Toggle 1:1 actual size", "Масштаб 1:1")
                            }
                            DoubleClickAction::None => tr("None", "Нет"),
                        })
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(
                                    &mut config.window.double_click_action,
                                    DoubleClickAction::ToggleWindowMode,
                                    tr("Toggle window mode", "Переключать режим окна"),
                                )
                                .clicked()
                            {
                                config_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut config.window.double_click_action,
                                    DoubleClickAction::ToggleFitToWindow,
                                    tr("Toggle fit to window", "Вписывать в окно"),
                                )
                                .clicked()
                            {
                                config_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut config.window.double_click_action,
                                    DoubleClickAction::ToggleActualSize,
                                    tr("Toggle 1:1 actual size", "Масштаб 1:1"),
                                )
                                .clicked()
                            {
                                config_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut config.window.double_click_action,
                                    DoubleClickAction::None,
                                    tr("None", "Нет"),
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
                        tr(
                            "Transparent background in windowed mode",
                            "Прозрачный фон в оконном режиме",
                        ),
                    )
                    .changed()
                {
                    config_changed = true;
                }

                if ui
                    .checkbox(
                        &mut config.window.show_checkerboard_for_transparent,
                        tr(
                            "Show checkerboard for transparency",
                            "Шахматный фон для прозрачности",
                        ),
                    )
                    .changed()
                {
                    config_changed = true;
                }

                if ui
                    .checkbox(
                        &mut config.window.auto_hide_ui,
                        tr(
                            "Auto-hide controls when idle",
                            "Скрывать элементы управления при бездействии",
                        ),
                    )
                    .changed()
                {
                    config_changed = true;
                }

                ui.separator();

                // --- 2. RENDERING & ZOOM SECTION ---
                ui.heading(tr("Rendering & Zoom", "Отображение и масштаб"));
                ui.horizontal(|ui| {
                    ui.label(tr("Texture filtering:", "Фильтрация:"));
                    if ui
                        .selectable_label(
                            config.rendering.filter_mode == FilterMode::Auto,
                            tr("Auto", "Авто"),
                        )
                        .clicked()
                    {
                        config.rendering.filter_mode = FilterMode::Auto;
                        config_changed = true;
                    }
                    if ui
                        .selectable_label(
                            config.rendering.filter_mode == FilterMode::Nearest,
                            tr("Nearest (pixel art)", "Без сглаживания"),
                        )
                        .clicked()
                    {
                        config.rendering.filter_mode = FilterMode::Nearest;
                        config_changed = true;
                    }
                    if ui
                        .selectable_label(
                            config.rendering.filter_mode == FilterMode::Bilinear,
                            tr("Bilinear (smooth)", "Билинейная"),
                        )
                        .clicked()
                    {
                        config.rendering.filter_mode = FilterMode::Bilinear;
                        config_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(tr("Zoom speed:", "Скорость масштаба:"));
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
                        tr("Zoom toward pointer", "Масштабировать к курсору"),
                    )
                    .changed()
                {
                    config_changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.rendering.reverse_zoom_direction,
                        tr("Invert mouse-wheel zoom", "Инвертировать колесо мыши"),
                    )
                    .changed()
                {
                    config_changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.rendering.smooth_pan,
                        tr("Short panning inertia", "Короткая инерция перемещения"),
                    )
                    .changed()
                {
                    config_changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.rendering.animate_image_transitions,
                        tr(
                            "Animate image changes",
                            "Анимация перелистывания изображений",
                        ),
                    )
                    .changed()
                {
                    config_changed = true;
                }
                ui.add_enabled_ui(config.rendering.animate_image_transitions, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tr("Transition duration:", "Длительность перехода:"));
                        if ui
                            .add(
                                egui::Slider::new(
                                    &mut config.rendering.image_transition_ms,
                                    80.0..=500.0,
                                )
                                .suffix(tr(" ms", " мс")),
                            )
                            .changed()
                        {
                            config_changed = true;
                        }
                    });
                });

                ui.separator();

                // --- 3. NAVIGATION SECTION ---
                ui.heading(tr("Navigation & Controls", "Навигация и управление"));
                if ui
                    .checkbox(
                        &mut config.navigation.loop_folder,
                        tr("Loop folder navigation", "Циклическая навигация по папке"),
                    )
                    .changed()
                {
                    config_changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.navigation.edge_buttons_enabled,
                        tr(
                            "Show edge navigation buttons",
                            "Показывать кнопки навигации по краям",
                        ),
                    )
                    .changed()
                {
                    config_changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.navigation.animate_buttons,
                        tr(
                            "Animate navigation buttons",
                            "Анимация кнопок перелистывания",
                        ),
                    )
                    .changed()
                {
                    config_changed = true;
                }

                ui.separator();

                // --- 4. FILMSTRIP (BOTTOM GALLERY) SECTION ---
                ui.heading(tr("Filmstrip", "Карусель"));
                ui.horizontal(|ui| {
                    ui.label(tr("Visibility:", "Видимость:"));
                    if ui
                        .selectable_label(
                            config.filmstrip.visibility == FilmstripVisibility::Hover,
                            tr("On hover", "При наведении"),
                        )
                        .clicked()
                    {
                        config.filmstrip.visibility = FilmstripVisibility::Hover;
                        config_changed = true;
                    }
                    if ui
                        .selectable_label(
                            config.filmstrip.visibility == FilmstripVisibility::Always,
                            tr("Always", "Всегда"),
                        )
                        .clicked()
                    {
                        config.filmstrip.visibility = FilmstripVisibility::Always;
                        config_changed = true;
                    }
                    if ui
                        .selectable_label(
                            config.filmstrip.visibility == FilmstripVisibility::Hidden,
                            tr("Hidden", "Скрыта"),
                        )
                        .clicked()
                    {
                        config.filmstrip.visibility = FilmstripVisibility::Hidden;
                        config_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(tr("Thumbnail size (px):", "Размер миниатюр (пкс):"));
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
                        tr(
                            "Scroll to active image",
                            "Прокручивать к активному изображению",
                        ),
                    )
                    .changed()
                {
                    config_changed = true;
                }
                if ui
                    .checkbox(
                        &mut config.filmstrip.coverflow_effect,
                        tr(
                            "Enlarge thumbnails near center",
                            "Увеличивать превью ближе к центру",
                        ),
                    )
                    .changed()
                {
                    config_changed = true;
                }

                ui.separator();

                // --- 5. CACHE & PERFORMANCE SECTION ---
                ui.heading(tr("Cache & Preloading", "Кеш и предзагрузка"));
                ui.horizontal(|ui| {
                    ui.label(tr("RAM cache limit (MB):", "Лимит RAM-кеша (МБ):"));
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
                    ui.label(tr("Prefetch adjacent images:", "Предзагружать соседние:"));
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
                    if ui.button(tr("Save", "Сохранить")).clicked() {
                        let _ = config.save();
                        should_close = true;
                    }

                    if ui.button(tr("Reset", "Сбросить")).clicked() {
                        *config = ViewerConfig::default();
                        let _ = config.save();
                        config_changed = true;
                    }

                    if ui.button(tr("Close", "Закрыть")).clicked() {
                        should_close = true;
                    }
                });
            });

        self.is_open = open && !should_close;
        config_changed
    }
}
