use crate::config::{
    DoubleClickAction, FilmstripVisibility, FilterMode, Language, ViewerConfig, WindowMode,
};
use egui::{Context, Id, ScrollArea, Vec2, Window};

const SETTINGS_DIALOG_ID: &str = "viewlume.settings.dialog";
const DIALOG_SAFETY_INSET: f32 = 8.0;
const NORMAL_OUTER_DIALOG_SIZE: Vec2 = Vec2::new(560.0, 400.0);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SettingsChanges {
    pub any: bool,
    pub filter: bool,
    pub cache: bool,
    pub window_mode: bool,
}

impl SettingsChanges {
    fn changed(&mut self) {
        self.any = true;
    }

    fn filter_changed(&mut self) {
        self.changed();
        self.filter = true;
    }

    fn cache_changed(&mut self) {
        self.changed();
        self.cache = true;
    }

    fn window_mode_changed(&mut self) {
        self.changed();
        self.window_mode = true;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SettingsAction {
    #[default]
    None,
    Close,
    Save,
    Reset,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SettingsOutcome {
    pub changes: SettingsChanges,
    pub action: SettingsAction,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum SettingsCategory {
    #[default]
    General,
    Rendering,
    Navigation,
    Filmstrip,
    Cache,
}

impl SettingsCategory {
    const ALL: [Self; 5] = [
        Self::General,
        Self::Rendering,
        Self::Navigation,
        Self::Filmstrip,
        Self::Cache,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Rendering => "rendering",
            Self::Navigation => "navigation",
            Self::Filmstrip => "filmstrip",
            Self::Cache => "cache",
        }
    }

    fn label(self, ru: bool) -> &'static str {
        match (self, ru) {
            (Self::General, false) => "General",
            (Self::General, true) => "Основное",
            (Self::Rendering, false) => "Rendering",
            (Self::Rendering, true) => "Отображение",
            (Self::Navigation, false) => "Navigation",
            (Self::Navigation, true) => "Навигация",
            (Self::Filmstrip, false) => "Filmstrip",
            (Self::Filmstrip, true) => "Карусель",
            (Self::Cache, false) => "Cache",
            (Self::Cache, true) => "Кеш",
        }
    }

    fn control_id(self, field: &'static str) -> Id {
        Id::new(("viewlume.settings.control", self.id(), field))
    }
}

#[derive(Default)]
pub struct SettingsDialog {
    pub is_open: bool,
    category: SettingsCategory,
    reset_pending: bool,
    reset_focus_requested: bool,
    save_error: Option<SaveError>,
}

#[derive(Clone)]
struct SaveError {
    after_reset: bool,
    detail: String,
}

impl SaveError {
    fn new(after_reset: bool, detail: String) -> Self {
        const MAX_DETAIL_CHARS: usize = 160;
        let mut bounded = detail.chars().take(MAX_DETAIL_CHARS).collect::<String>();
        if detail.chars().count() > MAX_DETAIL_CHARS {
            bounded.push('…');
        }
        Self {
            after_reset,
            detail: bounded,
        }
    }
}

impl SettingsDialog {
    pub fn show(
        &mut self,
        ctx: &Context,
        config: &mut ViewerConfig,
        opaque_window: bool,
    ) -> SettingsOutcome {
        if !self.is_open {
            return SettingsOutcome::default();
        }

        let mut outcome = SettingsOutcome::default();
        let mut open = self.is_open;
        let mut should_close = false;
        let available = ctx.screen_rect().shrink(DIALOG_SAFETY_INSET);
        let window_chrome = egui::Frame::window(&ctx.style()).inner_margin.sum();
        let compact = available.height() < NORMAL_OUTER_DIALOG_SIZE.y;
        // `Window` adds the frame margins and title bar after its content-size
        // constraint. Reserve three additional vertical frame margins so the
        // resulting outer rect still fits the logical inset on compact screens.
        let max_size = (available.size() - Vec2::new(window_chrome.x, window_chrome.y * 4.0))
            .max(Vec2::splat(1.0));
        let ru = config.general.language.is_russian();
        let tr = |english: &'static str, russian: &'static str| if ru { russian } else { english };

        self.handle_escape(ctx, &mut open, &mut outcome);

        let window = Window::new(tr("Settings", "Настройки"))
            .id(Id::new(SETTINGS_DIALOG_ID))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size((NORMAL_OUTER_DIALOG_SIZE - window_chrome).min(max_size))
            .min_size(Vec2::ZERO)
            .max_size(max_size)
            .constrain_to(available);
        let window = if compact {
            window.fixed_size(max_size)
        } else {
            window
        };
        window.show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);

            ui.add_enabled_ui(!self.reset_pending, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for category in SettingsCategory::ALL {
                        ui.push_id(category.control_id("tab"), |ui| {
                            ui.selectable_value(&mut self.category, category, category.label(ru));
                        });
                    }
                });
            });
            ui.separator();

            let reserved_footer_height =
                self.footer_height(ui, self.reset_pending, self.save_error.is_some());
            let body_height = (ui.available_height() - reserved_footer_height).max(0.0);
            let (body_rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), body_height),
                egui::Sense::hover(),
            );
            let mut body_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(body_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            body_ui.add_enabled_ui(!self.reset_pending, |ui| {
                ScrollArea::vertical()
                    .id_salt(("viewlume.settings.scroll", self.category.id()))
                    .auto_shrink([false, false])
                    .max_height(body_height)
                    .show(ui, |ui| {
                        ui.push_id(self.category.control_id("content"), |ui| {
                            self.category_content(
                                ui,
                                config,
                                opaque_window,
                                ru,
                                &mut outcome.changes,
                            );
                        });
                    });
            });

            if let Some(error) = &self.save_error {
                ui.separator();
                let heading = if error.after_reset {
                    tr(
                        "Defaults are applied, but could not be saved.",
                        "Настройки по умолчанию применены, но сохранить их не удалось.",
                    )
                } else {
                    tr("Could not save settings", "Не удалось сохранить настройки")
                };
                ui.colored_label(egui::Color32::from_rgb(248, 113, 113), heading);
                ui.add_sized(
                    Vec2::new(
                        ui.available_width(),
                        ui.text_style_height(&egui::TextStyle::Body)
                            * if compact { 1.0 } else { 2.0 },
                    ),
                    if compact {
                        egui::Label::new(&error.detail).truncate()
                    } else {
                        egui::Label::new(&error.detail).wrap()
                    },
                );
            }

            ui.separator();
            if self.reset_pending {
                self.reset_footer(ui, config, ru, &mut outcome);
            } else {
                self.normal_footer(ui, config, ru, &mut should_close, &mut outcome);
            }
        });

        self.is_open = open && !should_close;
        if !self.is_open && outcome.action == SettingsAction::None {
            outcome.action = SettingsAction::Close;
        }
        if !self.is_open {
            self.reset_pending = false;
            self.reset_focus_requested = false;
            self.save_error = None;
        }
        outcome
    }

    fn footer_height(&self, ui: &egui::Ui, reset_pending: bool, has_error: bool) -> f32 {
        let line_height = ui.text_style_height(&egui::TextStyle::Body);
        let button_height = ui.spacing().interact_size.y;
        let error_height = if has_error {
            line_height * 4.0 + ui.spacing().item_spacing.y
        } else {
            0.0
        };
        let footer_height = if reset_pending {
            line_height * 3.0 + button_height + ui.spacing().item_spacing.y * 3.0
        } else {
            button_height + ui.spacing().item_spacing.y
        };
        error_height + footer_height + ui.spacing().item_spacing.y * 2.0
    }

    fn handle_escape(&mut self, ctx: &Context, open: &mut bool, outcome: &mut SettingsOutcome) {
        let initial_escape = ctx.input(|input| {
            input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Escape,
                        pressed: true,
                        repeat: false,
                        ..
                    }
                )
            })
        });
        if !initial_escape {
            return;
        }

        if ctx.memory(|memory| memory.any_popup_open()) {
            ctx.memory_mut(|memory| memory.close_popup());
        } else if self.reset_pending {
            self.reset_pending = false;
            self.reset_focus_requested = false;
        } else if !ctx.wants_keyboard_input() {
            *open = false;
            outcome.action = SettingsAction::Close;
        }
    }

    fn save_result(&mut self, after_reset: bool, result: Result<(), String>) -> bool {
        match result {
            Ok(()) => {
                self.save_error = None;
                true
            }
            Err(detail) => {
                self.save_error = Some(SaveError::new(after_reset, detail));
                false
            }
        }
    }

    fn reset_with_save(
        &mut self,
        config: &mut ViewerConfig,
        save: impl FnOnce(&ViewerConfig) -> Result<(), String>,
    ) {
        *config = ViewerConfig::default();
        self.reset_pending = false;
        self.reset_focus_requested = false;
        self.save_result(true, save(config));
    }

    fn reset_changes() -> SettingsChanges {
        SettingsChanges {
            any: true,
            filter: true,
            cache: true,
            window_mode: true,
        }
    }

    fn category_content(
        &mut self,
        ui: &mut egui::Ui,
        config: &mut ViewerConfig,
        opaque_window: bool,
        ru: bool,
        changes: &mut SettingsChanges,
    ) {
        match self.category {
            SettingsCategory::General => {
                self.general_content(ui, config, opaque_window, ru, changes)
            }
            SettingsCategory::Rendering => self.rendering_content(ui, config, ru, changes),
            SettingsCategory::Navigation => self.navigation_content(ui, config, ru, changes),
            SettingsCategory::Filmstrip => self.filmstrip_content(ui, config, ru, changes),
            SettingsCategory::Cache => self.cache_content(ui, config, ru, changes),
        }
    }

    fn general_content(
        &self,
        ui: &mut egui::Ui,
        config: &mut ViewerConfig,
        opaque_window: bool,
        ru: bool,
        changes: &mut SettingsChanges,
    ) {
        let tr = |english, russian| if ru { russian } else { english };
        ui.heading(tr("Language", "Язык"));
        ui.label(tr("Interface language:", "Язык интерфейса:"));
        ui.push_id(SettingsCategory::General.control_id("language"), |ui| {
            ui.horizontal_wrapped(|ui| {
                for (language, label) in [
                    (Language::Auto, tr("System", "Системный")),
                    (Language::English, "English"),
                    (Language::Russian, "Русский"),
                ] {
                    if ui
                        .selectable_value(&mut config.general.language, language, label)
                        .changed()
                    {
                        changes.changed();
                    }
                }
            });
        });

        ui.separator();
        ui.heading(tr("Window & Display", "Окно и экран"));
        ui.label(tr("Default mode:", "Режим по умолчанию:"));
        ui.push_id(SettingsCategory::General.control_id("window_mode"), |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .selectable_value(
                        &mut config.window.mode,
                        WindowMode::Overlay,
                        tr("Fullscreen overlay", "Полноэкранный оверлей"),
                    )
                    .changed()
                {
                    changes.window_mode_changed();
                }
                if ui
                    .selectable_value(
                        &mut config.window.mode,
                        WindowMode::Windowed,
                        tr("Windowed", "Оконный"),
                    )
                    .changed()
                {
                    changes.window_mode_changed();
                }
            });
        });

        ui.label(tr("Double-click canvas:", "Двойной клик:"));
        egui::ComboBox::from_id_salt(SettingsCategory::General.control_id("double_click"))
            .selected_text(match config.window.double_click_action {
                DoubleClickAction::ToggleWindowMode => {
                    tr("Toggle window mode", "Переключать режим окна")
                }
                DoubleClickAction::ToggleFitToWindow => {
                    tr("Toggle fit to window", "Вписывать в окно")
                }
                DoubleClickAction::ToggleActualSize => tr("Toggle 1:1 actual size", "Масштаб 1:1"),
                DoubleClickAction::None => tr("None", "Нет"),
            })
            .show_ui(ui, |ui| {
                for (value, label) in [
                    (
                        DoubleClickAction::ToggleWindowMode,
                        tr("Toggle window mode", "Переключать режим окна"),
                    ),
                    (
                        DoubleClickAction::ToggleFitToWindow,
                        tr("Toggle fit to window", "Вписывать в окно"),
                    ),
                    (
                        DoubleClickAction::ToggleActualSize,
                        tr("Toggle 1:1 actual size", "Масштаб 1:1"),
                    ),
                    (DoubleClickAction::None, tr("None", "Нет")),
                ] {
                    if ui
                        .selectable_value(&mut config.window.double_click_action, value, label)
                        .changed()
                    {
                        changes.changed();
                    }
                }
            });

        ui.add_enabled_ui(!opaque_window, |ui| {
            self.checkbox(
                ui,
                SettingsCategory::General.control_id("transparent_background"),
                &mut config.window.transparent_windowed_background,
                tr(
                    "Transparent background in windowed mode",
                    "Прозрачный фон в оконном режиме",
                ),
                changes,
            );
        });
        if opaque_window {
            ui.label(tr(
                "Transparent window backgrounds are unavailable in opaque compatibility mode.",
                "Прозрачный фон окна недоступен в режиме совместимости с непрозрачным окном.",
            ));
        }

        self.checkbox(
            ui,
            SettingsCategory::General.control_id("checkerboard"),
            &mut config.window.show_checkerboard_for_transparent,
            tr(
                "Show checkerboard for transparency",
                "Шахматный фон для прозрачности",
            ),
            changes,
        );
        self.checkbox(
            ui,
            SettingsCategory::General.control_id("auto_hide"),
            &mut config.window.auto_hide_ui,
            tr(
                "Auto-hide controls when idle",
                "Скрывать элементы управления при бездействии",
            ),
            changes,
        );
    }

    fn rendering_content(
        &self,
        ui: &mut egui::Ui,
        config: &mut ViewerConfig,
        ru: bool,
        changes: &mut SettingsChanges,
    ) {
        let tr = |english, russian| if ru { russian } else { english };
        ui.heading(tr("Rendering & Zoom", "Отображение и масштаб"));
        ui.label(tr("Texture filtering:", "Фильтрация:"));
        ui.push_id(SettingsCategory::Rendering.control_id("filter"), |ui| {
            ui.horizontal_wrapped(|ui| {
                for (mode, english, russian) in [
                    (FilterMode::Auto, "Auto", "Авто"),
                    (
                        FilterMode::Nearest,
                        "Nearest (pixel art)",
                        "Без сглаживания",
                    ),
                    (FilterMode::Bilinear, "Bilinear (smooth)", "Билинейная"),
                ] {
                    if ui
                        .selectable_value(
                            &mut config.rendering.filter_mode,
                            mode,
                            tr(english, russian),
                        )
                        .changed()
                    {
                        changes.filter_changed();
                    }
                }
            });
        });

        self.slider_row(
            ui,
            SettingsCategory::Rendering.control_id("zoom_factor"),
            tr("Zoom speed:", "Скорость масштаба:"),
            |ui| {
                ui.add(
                    egui::Slider::new(&mut config.rendering.zoom_factor, 1.05..=1.50).step_by(0.01),
                )
                .changed()
            },
            changes,
        );
        self.checkbox(
            ui,
            SettingsCategory::Rendering.control_id("zoom_towards_cursor"),
            &mut config.rendering.zoom_towards_cursor,
            tr("Zoom toward pointer", "Масштабировать к курсору"),
            changes,
        );
        self.checkbox(
            ui,
            SettingsCategory::Rendering.control_id("reverse_zoom"),
            &mut config.rendering.reverse_zoom_direction,
            tr("Invert mouse-wheel zoom", "Инвертировать колесо мыши"),
            changes,
        );
        self.checkbox(
            ui,
            SettingsCategory::Rendering.control_id("smooth_pan"),
            &mut config.rendering.smooth_pan,
            tr("Short panning inertia", "Короткая инерция перемещения"),
            changes,
        );
        self.checkbox(
            ui,
            SettingsCategory::Rendering.control_id("animate_transitions"),
            &mut config.rendering.animate_image_transitions,
            tr(
                "Animate image changes",
                "Анимация перелистывания изображений",
            ),
            changes,
        );
        ui.add_enabled_ui(config.rendering.animate_image_transitions, |ui| {
            self.slider_row(
                ui,
                SettingsCategory::Rendering.control_id("transition_duration"),
                tr("Transition duration:", "Длительность перехода:"),
                |ui| {
                    ui.add(
                        egui::Slider::new(&mut config.rendering.image_transition_ms, 80.0..=500.0)
                            .suffix(tr(" ms", " мс")),
                    )
                    .changed()
                },
                changes,
            );
        });
    }

    fn navigation_content(
        &self,
        ui: &mut egui::Ui,
        config: &mut ViewerConfig,
        ru: bool,
        changes: &mut SettingsChanges,
    ) {
        let tr = |english, russian| if ru { russian } else { english };
        ui.heading(tr("Navigation & Controls", "Навигация и управление"));
        self.checkbox(
            ui,
            SettingsCategory::Navigation.control_id("loop_folder"),
            &mut config.navigation.loop_folder,
            tr("Loop folder navigation", "Циклическая навигация по папке"),
            changes,
        );
        self.checkbox(
            ui,
            SettingsCategory::Navigation.control_id("edge_buttons"),
            &mut config.navigation.edge_buttons_enabled,
            tr(
                "Show edge navigation buttons",
                "Показывать кнопки навигации по краям",
            ),
            changes,
        );
        self.checkbox(
            ui,
            SettingsCategory::Navigation.control_id("animate_buttons"),
            &mut config.navigation.animate_buttons,
            tr(
                "Animate navigation buttons",
                "Анимация кнопок перелистывания",
            ),
            changes,
        );
    }

    fn filmstrip_content(
        &self,
        ui: &mut egui::Ui,
        config: &mut ViewerConfig,
        ru: bool,
        changes: &mut SettingsChanges,
    ) {
        let tr = |english, russian| if ru { russian } else { english };
        ui.heading(tr("Filmstrip", "Карусель"));
        ui.label(tr("Visibility:", "Видимость:"));
        ui.push_id(SettingsCategory::Filmstrip.control_id("visibility"), |ui| {
            ui.horizontal_wrapped(|ui| {
                for (value, english, russian) in [
                    (FilmstripVisibility::Hover, "On hover", "При наведении"),
                    (FilmstripVisibility::Always, "Always", "Всегда"),
                    (FilmstripVisibility::Hidden, "Hidden", "Скрыта"),
                ] {
                    if ui
                        .selectable_value(
                            &mut config.filmstrip.visibility,
                            value,
                            tr(english, russian),
                        )
                        .changed()
                    {
                        changes.changed();
                    }
                }
            });
        });
        self.slider_row(
            ui,
            SettingsCategory::Filmstrip.control_id("thumbnail_size"),
            tr("Thumbnail size (px):", "Размер миниатюр (пкс):"),
            |ui| {
                ui.add(egui::Slider::new(
                    &mut config.filmstrip.thumbnail_size,
                    48..=128,
                ))
                .changed()
            },
            changes,
        );
        self.checkbox(
            ui,
            SettingsCategory::Filmstrip.control_id("scroll_active"),
            &mut config.filmstrip.auto_scroll_to_active,
            tr(
                "Scroll to active image",
                "Прокручивать к активному изображению",
            ),
            changes,
        );
        self.checkbox(
            ui,
            SettingsCategory::Filmstrip.control_id("coverflow"),
            &mut config.filmstrip.coverflow_effect,
            tr(
                "Enlarge thumbnails near center",
                "Увеличивать превью ближе к центру",
            ),
            changes,
        );
    }

    fn cache_content(
        &self,
        ui: &mut egui::Ui,
        config: &mut ViewerConfig,
        ru: bool,
        changes: &mut SettingsChanges,
    ) {
        let tr = |english, russian| if ru { russian } else { english };
        ui.heading(tr("Cache & Preloading", "Кеш и предзагрузка"));
        ui.label(tr(
            "Decoded-image cache capacity (MB); active images, decoding, animation frames, and GPU textures are separate.",
            "Ёмкость кеша декодированных изображений (МБ); активные изображения, декодирование, кадры анимации и текстуры GPU учитываются отдельно.",
        ));
        let mut cache_changed = false;
        self.slider_row(
            ui,
            SettingsCategory::Cache.control_id("capacity"),
            tr("Cache capacity (MB):", "Ёмкость кеша (МБ):"),
            |ui| {
                let changed = ui
                    .add(
                        egui::Slider::new(&mut config.cache.max_ram_cache_mb, 256..=4096)
                            .step_by(128.0),
                    )
                    .changed();
                cache_changed |= changed;
                changed
            },
            changes,
        );
        if cache_changed {
            changes.cache_changed();
        }
        self.slider_row(
            ui,
            SettingsCategory::Cache.control_id("prefetch"),
            tr("Prefetch adjacent images:", "Предзагружать соседние:"),
            |ui| {
                ui.add(egui::Slider::new(&mut config.cache.prefetch_count, 0..=6))
                    .changed()
            },
            changes,
        );
    }

    fn checkbox(
        &self,
        ui: &mut egui::Ui,
        id: Id,
        value: &mut bool,
        label: &str,
        changes: &mut SettingsChanges,
    ) {
        ui.push_id(id, |ui| {
            if ui.checkbox(value, label).changed() {
                changes.changed();
            }
        });
    }

    fn slider_row(
        &self,
        ui: &mut egui::Ui,
        id: Id,
        label: &str,
        add_slider: impl FnOnce(&mut egui::Ui) -> bool,
        changes: &mut SettingsChanges,
    ) {
        ui.push_id(id, |ui| {
            let narrow = ui.available_width() < 300.0;
            if narrow {
                ui.label(label);
                if add_slider(ui) {
                    changes.changed();
                }
            } else {
                ui.horizontal(|ui| {
                    ui.label(label);
                    if add_slider(ui) {
                        changes.changed();
                    }
                });
            }
        });
    }

    fn normal_footer(
        &mut self,
        ui: &mut egui::Ui,
        config: &mut ViewerConfig,
        ru: bool,
        should_close: &mut bool,
        outcome: &mut SettingsOutcome,
    ) {
        let tr = |english, russian| if ru { russian } else { english };
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("Reset", "Сбросить")).clicked() {
                self.reset_pending = true;
                self.reset_focus_requested = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Close", "Закрыть")).clicked() {
                    *should_close = true;
                    outcome.action = SettingsAction::Close;
                }
                let retrying_reset_save = self
                    .save_error
                    .as_ref()
                    .is_some_and(|error| error.after_reset);
                if ui.button(tr("Save", "Сохранить")).clicked()
                    && self.save_result(retrying_reset_save, config.save())
                {
                    *should_close = true;
                    outcome.action = SettingsAction::Save;
                }
            });
        });
    }

    fn reset_footer(
        &mut self,
        ui: &mut egui::Ui,
        config: &mut ViewerConfig,
        ru: bool,
        outcome: &mut SettingsOutcome,
    ) {
        let tr = |english, russian| if ru { russian } else { english };
        ui.label(
            egui::RichText::new(tr("Reset all settings?", "Сбросить все настройки?")).strong(),
        );
        ui.label(tr(
            "This resets every category to its defaults and saves the changes.",
            "Все категории будут сброшены к значениям по умолчанию, а изменения — сохранены.",
        ));
        ui.horizontal(|ui| {
            let cancel = ui.button(tr("Cancel", "Отмена"));
            if self.reset_focus_requested {
                cancel.request_focus();
                self.reset_focus_requested = false;
            }
            if cancel.clicked() {
                self.reset_pending = false;
                self.reset_focus_requested = false;
            }
            if ui.button(tr("Reset", "Сбросить")).clicked() {
                self.reset_with_save(config, ViewerConfig::save);
                outcome.changes = Self::reset_changes();
                outcome.action = SettingsAction::Reset;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn render_dialog(
        ctx: &Context,
        dialog: &mut SettingsDialog,
        config: &mut ViewerConfig,
        screen: egui::Rect,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                assert_eq!(dialog.show(ctx, config, false).action, SettingsAction::None);
            },
        );
    }

    fn assert_dialog_in_bounds(ctx: &Context, bounds: egui::Rect) {
        let rect = ctx
            .memory(|memory| memory.area_rect(Id::new(SETTINGS_DIALOG_ID)))
            .expect("settings window rect");
        assert!(
            bounds.contains_rect(rect),
            "settings rect outside bounds: {rect:?}"
        );
    }

    #[test]
    fn categories_have_stable_nonlocalized_ids() {
        assert_eq!(SettingsCategory::General.id(), "general");
        assert_eq!(SettingsCategory::Rendering.id(), "rendering");
        assert_eq!(SettingsCategory::Navigation.id(), "navigation");
        assert_eq!(SettingsCategory::Filmstrip.id(), "filmstrip");
        assert_eq!(SettingsCategory::Cache.id(), "cache");

        let control_ids = [
            SettingsCategory::General.control_id("language"),
            SettingsCategory::General.control_id("window_mode"),
            SettingsCategory::General.control_id("double_click"),
            SettingsCategory::General.control_id("transparent_background"),
            SettingsCategory::General.control_id("checkerboard"),
            SettingsCategory::General.control_id("auto_hide"),
            SettingsCategory::Rendering.control_id("filter"),
            SettingsCategory::Rendering.control_id("zoom_factor"),
            SettingsCategory::Rendering.control_id("zoom_towards_cursor"),
            SettingsCategory::Rendering.control_id("reverse_zoom"),
            SettingsCategory::Rendering.control_id("smooth_pan"),
            SettingsCategory::Rendering.control_id("animate_transitions"),
            SettingsCategory::Rendering.control_id("transition_duration"),
            SettingsCategory::Navigation.control_id("loop_folder"),
            SettingsCategory::Navigation.control_id("edge_buttons"),
            SettingsCategory::Navigation.control_id("animate_buttons"),
            SettingsCategory::Filmstrip.control_id("visibility"),
            SettingsCategory::Filmstrip.control_id("thumbnail_size"),
            SettingsCategory::Filmstrip.control_id("scroll_active"),
            SettingsCategory::Filmstrip.control_id("coverflow"),
            SettingsCategory::Cache.control_id("capacity"),
            SettingsCategory::Cache.control_id("prefetch"),
        ];
        assert_eq!(
            control_ids.len(),
            control_ids.iter().collect::<HashSet<_>>().len()
        );
    }

    #[test]
    fn outcome_marks_only_affected_runtime_settings() {
        let mut changes = SettingsChanges::default();
        changes.changed();
        assert_eq!(
            changes,
            SettingsChanges {
                any: true,
                ..Default::default()
            }
        );

        changes.filter_changed();
        assert!(changes.filter);
        assert!(!changes.cache);

        changes.cache_changed();
        assert!(changes.cache);

        let mut window_mode = SettingsChanges::default();
        window_mode.window_mode_changed();
        assert_eq!(
            window_mode,
            SettingsChanges {
                any: true,
                window_mode: true,
                ..Default::default()
            }
        );

        assert_eq!(
            SettingsDialog::reset_changes(),
            SettingsChanges {
                any: true,
                filter: true,
                cache: true,
                window_mode: true,
            }
        );
    }

    #[test]
    fn escape_prioritizes_popup_reset_text_edit_then_dialog_close() {
        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };

        let mut popup_dialog = SettingsDialog {
            reset_pending: true,
            ..Default::default()
        };
        let popup_ctx = Context::default();
        let _ = popup_ctx.run(
            egui::RawInput {
                events: vec![escape.clone()],
                ..Default::default()
            },
            |ctx| {
                ctx.memory_mut(|memory| memory.open_popup(Id::new("test.popup")));
                let mut open = true;
                let mut outcome = SettingsOutcome::default();
                popup_dialog.handle_escape(ctx, &mut open, &mut outcome);
                assert!(open);
                assert!(popup_dialog.reset_pending);
                assert!(!ctx.memory(|memory| memory.any_popup_open()));
                assert_eq!(outcome.action, SettingsAction::None);
            },
        );

        let reset_ctx = Context::default();
        let _ = reset_ctx.run(
            egui::RawInput {
                events: vec![escape.clone()],
                ..Default::default()
            },
            |ctx| {
                let mut open = true;
                let mut outcome = SettingsOutcome::default();
                popup_dialog.handle_escape(ctx, &mut open, &mut outcome);
                assert!(open);
                assert!(!popup_dialog.reset_pending);
                assert_eq!(outcome.action, SettingsAction::None);
            },
        );

        let mut focused_dialog = SettingsDialog::default();
        let focused_ctx = Context::default();
        let _ = focused_ctx.run(
            egui::RawInput {
                events: vec![escape.clone()],
                ..Default::default()
            },
            |ctx| {
                ctx.memory_mut(|memory| memory.request_focus(Id::new("test.text_edit")));
                let mut open = true;
                let mut outcome = SettingsOutcome::default();
                focused_dialog.handle_escape(ctx, &mut open, &mut outcome);
                assert!(open);
                assert_eq!(outcome.action, SettingsAction::None);
            },
        );

        let close_ctx = Context::default();
        let _ = close_ctx.run(
            egui::RawInput {
                events: vec![escape.clone()],
                ..Default::default()
            },
            |ctx| {
                let mut open = true;
                let mut outcome = SettingsOutcome::default();
                focused_dialog.handle_escape(ctx, &mut open, &mut outcome);
                assert!(!open);
                assert_eq!(outcome.action, SettingsAction::Close);
            },
        );

        let repeat_ctx = Context::default();
        let _ = repeat_ctx.run(
            egui::RawInput {
                events: vec![escape.clone()],
                ..Default::default()
            },
            |_| {},
        );
        let _ = repeat_ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| {
                let mut open = true;
                let mut outcome = SettingsOutcome::default();
                focused_dialog.handle_escape(ctx, &mut open, &mut outcome);
                assert!(open);
                assert_eq!(outcome.action, SettingsAction::None);
            },
        );
    }

    #[test]
    fn compact_dialog_stays_inside_logical_bounds_for_every_category_and_language() {
        let ctx = Context::default();
        let mut dialog = SettingsDialog {
            is_open: true,
            ..Default::default()
        };
        let mut config = ViewerConfig::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(480.0, 320.0));
        let bounds = screen.shrink(DIALOG_SAFETY_INSET);

        for language in [Language::English, Language::Russian] {
            config.general.language = language;
            for category in SettingsCategory::ALL {
                dialog.category = category;
                for _ in 0..2 {
                    render_dialog(&ctx, &mut dialog, &mut config, screen);
                }
                assert_dialog_in_bounds(&ctx, bounds);
            }
        }

        dialog.reset_pending = true;
        dialog.reset_focus_requested = true;
        render_dialog(&ctx, &mut dialog, &mut config, screen);
        render_dialog(&ctx, &mut dialog, &mut config, screen);
        assert_dialog_in_bounds(&ctx, bounds);

        dialog.reset_pending = false;
        dialog.save_error = Some(SaveError::new(false, "access denied".repeat(20)));
        render_dialog(&ctx, &mut dialog, &mut config, screen);
        render_dialog(&ctx, &mut dialog, &mut config, screen);
        assert_dialog_in_bounds(&ctx, bounds);
        assert!(dialog.is_open);
    }

    #[test]
    fn reset_failure_keeps_defaults_applied_and_save_retry_clears_the_error() {
        let mut dialog = SettingsDialog::default();
        let mut config = ViewerConfig::default();
        config.cache.max_ram_cache_mb = 256;
        dialog.reset_with_save(&mut config, |_| Err("access denied".to_owned()));

        assert_eq!(
            config.cache.max_ram_cache_mb,
            ViewerConfig::default().cache.max_ram_cache_mb
        );
        assert!(dialog
            .save_error
            .as_ref()
            .is_some_and(|error| error.after_reset));

        let retrying_reset_save = dialog
            .save_error
            .as_ref()
            .is_some_and(|error| error.after_reset);
        assert!(!dialog.save_result(retrying_reset_save, Err("still denied".to_owned())));
        assert!(dialog
            .save_error
            .as_ref()
            .is_some_and(|error| error.after_reset));

        assert!(dialog.save_result(false, Ok(())));
        assert!(dialog.save_error.is_none());
    }

    #[test]
    fn opaque_compatibility_does_not_change_saved_transparency_preference() {
        let mut config = ViewerConfig::default();
        let original = config.window.transparent_windowed_background;
        let mut dialog = SettingsDialog {
            is_open: true,
            ..Default::default()
        };
        let ctx = Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            dialog.show(ctx, &mut config, true);
        });
        assert_eq!(config.window.transparent_windowed_background, original);
    }
}
