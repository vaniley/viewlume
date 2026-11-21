use crate::config::{
    DoubleClickAction, FilmstripVisibility, FilterMode, Language, ViewerConfig, WindowMode,
};
use egui::{Color32, ComboBox, Context, Id, Order, ScrollArea, Slider, Ui, Window};

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
enum Tab {
    #[default]
    General,
    Image,
    Navigation,
    Advanced,
}

type Translate = dyn Fn(&'static str, &'static str) -> &'static str;

#[derive(Default)]
pub struct SettingsDialog {
    draft: Option<ViewerConfig>,
    tab: Tab,
    confirm_reset: bool,
    save_error: Option<String>,
    editing_number: bool,
}

#[derive(PartialEq, Eq)]
enum Action {
    None,
    Cancel,
    Save,
}

impl SettingsDialog {
    pub fn is_open(&self) -> bool {
        self.draft.is_some()
    }

    pub fn open(&mut self, config: &ViewerConfig) {
        if !self.is_open() {
            self.draft = Some(config.clone());
            self.confirm_reset = false;
            self.save_error = None;
            self.editing_number = false;
        }
    }

    /// Return true only when the saved draft replaces the active settings.
    pub fn show(&mut self, ctx: &Context, config: &mut ViewerConfig) -> bool {
        let Some(draft) = self.draft.as_mut() else {
            return false;
        };
        if *draft == *config {
            self.save_error = None;
        }
        let ru = config.general.language.is_russian();
        let tr = move |english, russian| if ru { russian } else { english };
        let screen = ctx.screen_rect();
        let width = (screen.width() - 48.0).clamp(200.0, 464.0);
        let body_height = (screen.height() - 160.0).clamp(80.0, 285.0);
        let mut action = Action::None;
        let mut open = true;
        // Egui clears the focused text field before this pass on Escape. Let
        // that first press end the edit (or close a combo box), not the dialog.
        if !self.editing_number
            && ctx.dragged_id().is_none()
            && !ctx.memory(|mem| mem.any_popup_open())
            && ctx.input(|input| {
                input.events.iter().any(|event| {
                    matches!(event, egui::Event::Key { key: egui::Key::Escape, pressed: true, repeat: false, modifiers, .. }
                        if modifiers.is_none())
                })
            })
        {
            if self.confirm_reset {
                self.confirm_reset = false;
            } else {
                action = Action::Cancel;
            }
        }

        let backdrop = egui::Area::new(Id::new("settings-backdrop"))
            .order(Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha(128));
            });
        ctx.move_to_top(backdrop.response.layer_id);

        Window::new(tr("Settings", "Настройки"))
            .id(Id::new("viewer-settings"))
            .order(Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .collapsible(false)
            .auto_sized()
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.add_enabled_ui(!self.confirm_reset, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (tab, label) in [
                            (Tab::General, tr("General", "Общие")),
                            (Tab::Image, tr("Image", "Изображение")),
                            (Tab::Navigation, tr("Navigation", "Навигация")),
                            (Tab::Advanced, tr("Advanced", "Дополнительно")),
                        ] {
                            ui.selectable_value(&mut self.tab, tab, label);
                        }
                    });
                });
                ui.separator();

                ScrollArea::vertical()
                    .id_salt((self.tab, self.confirm_reset, self.save_error.is_some()))
                    .max_height(body_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.confirm_reset {
                            ui.heading(tr("Restore defaults?", "Восстановить настройки?"));
                            ui.label(tr(
                                "This replaces your edits with the default settings. Nothing changes until you save.",
                                "Текущие правки будут заменены настройками по умолчанию. Изменения вступят в силу после сохранения.",
                            ));
                        } else {
                            if let Some(error) = &self.save_error {
                                ui.colored_label(ui.visuals().error_fg_color, tr(
                                    "Could not save settings. Your changes are still here. Try again.",
                                    "Не удалось сохранить настройки. Ваши изменения не потеряны. Повторите попытку.",
                                ));
                                ui.add(egui::Label::new(error).wrap());
                            }
                            match self.tab {
                                Tab::General => general(ui, draft, &tr),
                                Tab::Image => image(ui, draft, &tr),
                                Tab::Navigation => navigation(ui, draft, &tr),
                                Tab::Advanced => advanced(ui, draft, &tr),
                            }
                        }
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if self.confirm_reset {
                        if ui.button(tr("Keep edits", "Оставить правки")).clicked() {
                            self.confirm_reset = false;
                        }
                        if ui.button(tr("Restore defaults", "Восстановить настройки")).clicked() {
                            *draft = ViewerConfig::default();
                            self.confirm_reset = false;
                            self.save_error = None;
                        }
                    } else {
                        if ui.button(tr("Restore defaults…", "Восстановить настройки…")).clicked() {
                            self.confirm_reset = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add_enabled(*draft != *config, egui::Button::new(tr("Save", "Сохранить"))).clicked() {
                                action = Action::Save;
                            }
                            if ui.button(tr("Cancel", "Отмена")).clicked() {
                                action = Action::Cancel;
                            }
                        });
                    }
                });
            });

        self.editing_number = ctx
            .memory(egui::Memory::focused)
            .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());

        if !open || action == Action::Cancel {
            self.draft = None;
            ctx.request_repaint();
        } else if action == Action::Save {
            ctx.request_repaint();
            return self.save_draft(config, ViewerConfig::save);
        }
        false
    }

    fn save_draft(
        &mut self,
        config: &mut ViewerConfig,
        save: impl FnOnce(&ViewerConfig) -> Result<(), String>,
    ) -> bool {
        let Some(draft) = self.draft.take() else {
            return false;
        };
        match save(&draft) {
            Ok(()) => {
                *config = draft;
                self.save_error = None;
                true
            }
            Err(error) => {
                self.draft = Some(draft);
                self.save_error = Some(error);
                false
            }
        }
    }
}

fn choice<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &'static str,
    label: &'static str,
    value: &mut T,
    options: &[(T, &'static str)],
) {
    let label = ui.label(label);
    let selected = options.iter().find(|(option, _)| option == value);
    ComboBox::from_id_salt(id)
        .selected_text(selected.map_or("", |(_, text)| *text))
        .show_ui(ui, |ui| {
            for &(option, text) in options {
                ui.selectable_value(value, option, text);
            }
        })
        .response
        .labelled_by(label.id);
}

fn slider(ui: &mut Ui, label: &'static str, slider: Slider<'_>) {
    let label = ui.label(label);
    ui.add(slider).labelled_by(label.id);
}

fn general(ui: &mut Ui, config: &mut ViewerConfig, tr: &Translate) {
    ui.heading(tr("Language", "Язык"));
    choice(
        ui,
        "language",
        tr("Interface language", "Язык интерфейса"),
        &mut config.general.language,
        &[
            (Language::English, "English"),
            (Language::Russian, "Русский"),
            (Language::Auto, tr("System", "Системный")),
        ],
    );
    ui.separator();
    ui.heading(tr("Window and controls", "Окно и управление"));
    choice(
        ui,
        "startup-mode",
        tr("Window mode at startup", "Режим окна при запуске"),
        &mut config.window.mode,
        &[
            (WindowMode::Windowed, tr("Windowed", "Оконный")),
            (
                WindowMode::Overlay,
                tr("Fullscreen overlay", "Полноэкранный оверлей"),
            ),
        ],
    );
    ui.label(tr(
        "To switch modes without restarting, close Settings and press F11.",
        "Чтобы переключить режим без перезапуска, закройте настройки и нажмите F11.",
    ));
    ui.checkbox(
        &mut config.window.auto_hide_ui,
        tr(
            "Hide controls when idle",
            "Скрывать элементы управления при бездействии",
        ),
    );
    ui.checkbox(
        &mut config.window.transparent_windowed_background,
        tr(
            "Transparent background in windowed mode",
            "Прозрачный фон в оконном режиме",
        ),
    );
}

fn image(ui: &mut Ui, config: &mut ViewerConfig, tr: &Translate) {
    ui.heading(tr("Image display", "Отображение изображения"));
    choice(
        ui,
        "texture-filter",
        tr("Texture filtering", "Фильтрация изображения"),
        &mut config.rendering.filter_mode,
        &[
            (FilterMode::Auto, tr("Automatic", "Автоматически")),
            (
                FilterMode::Nearest,
                tr("Nearest (sharp pixels)", "Без сглаживания"),
            ),
            (FilterMode::Bilinear, tr("Bilinear (smooth)", "Билинейная")),
        ],
    );
    ui.checkbox(
        &mut config.window.show_checkerboard_for_transparent,
        tr(
            "Show checkerboard behind transparent images",
            "Показывать шахматный фон за прозрачными изображениями",
        ),
    );
    ui.separator();
    ui.heading(tr("Zoom and pan", "Масштаб и перемещение"));
    choice(
        ui,
        "double-click",
        tr("Double-click action", "Действие двойного щелчка"),
        &mut config.window.double_click_action,
        &[
            (
                DoubleClickAction::ToggleWindowMode,
                tr("Switch fullscreen and windowed", "Переключить режим окна"),
            ),
            (
                DoubleClickAction::ToggleFitToWindow,
                tr("Fit to window", "Вписать в окно"),
            ),
            (
                DoubleClickAction::ToggleActualSize,
                tr("Show actual size (100%)", "Показать исходный размер (100%)"),
            ),
            (
                DoubleClickAction::None,
                tr("Do nothing", "Ничего не делать"),
            ),
        ],
    );
    slider(
        ui,
        tr("Zoom speed", "Скорость масштабирования"),
        Slider::new(&mut config.rendering.zoom_factor, 1.05..=1.50).step_by(0.01),
    );
    ui.checkbox(
        &mut config.rendering.zoom_towards_cursor,
        tr("Zoom toward pointer", "Масштабировать к курсору"),
    );
    ui.checkbox(
        &mut config.rendering.reverse_zoom_direction,
        tr(
            "Reverse wheel zoom",
            "Инвертировать масштабирование колёсиком",
        ),
    );
    ui.checkbox(
        &mut config.rendering.smooth_pan,
        tr(
            "Continue panning briefly after release",
            "Короткая инерция перемещения",
        ),
    );
    ui.separator();
    ui.heading(tr("Transitions", "Переходы"));
    ui.checkbox(
        &mut config.rendering.animate_image_transitions,
        tr("Animate image changes", "Анимировать смену изображений"),
    );
    ui.add_enabled_ui(config.rendering.animate_image_transitions, |ui| {
        slider(
            ui,
            tr("Transition duration", "Длительность перехода"),
            Slider::new(&mut config.rendering.image_transition_ms, 80.0..=500.0)
                .suffix(tr(" ms", " мс")),
        );
    });
}

fn navigation(ui: &mut Ui, config: &mut ViewerConfig, tr: &Translate) {
    ui.heading(tr("Folder navigation", "Навигация по папке"));
    ui.checkbox(
        &mut config.navigation.loop_folder,
        tr(
            "Wrap from last image to first",
            "Переходить между первым и последним изображениями",
        ),
    );
    ui.checkbox(
        &mut config.navigation.edge_buttons_enabled,
        tr("Show arrows at the edges", "Показывать стрелки по краям"),
    );
    ui.add_enabled(
        config.navigation.edge_buttons_enabled,
        egui::Checkbox::new(
            &mut config.navigation.animate_buttons,
            tr("Animate navigation arrows", "Анимировать стрелки навигации"),
        ),
    );
    ui.separator();
    ui.heading(tr("Filmstrip", "Карусель"));
    choice(
        ui,
        "filmstrip-visibility",
        tr("Show the filmstrip", "Показывать карусель"),
        &mut config.filmstrip.visibility,
        &[
            (FilmstripVisibility::Hover, tr("On hover", "При наведении")),
            (FilmstripVisibility::Always, tr("Always", "Всегда")),
            (FilmstripVisibility::Hidden, tr("Never", "Никогда")),
        ],
    );
    ui.add_enabled_ui(
        config.filmstrip.visibility != FilmstripVisibility::Hidden,
        |ui| {
            slider(
                ui,
                tr("Thumbnail size", "Размер миниатюр"),
                Slider::new(&mut config.filmstrip.thumbnail_size, 48..=128)
                    .suffix(tr(" px", " пкс")),
            );
            ui.checkbox(
                &mut config.filmstrip.auto_scroll_to_active,
                tr(
                    "Scroll to the current image",
                    "Прокручивать к текущему изображению",
                ),
            );
            ui.checkbox(
                &mut config.filmstrip.coverflow_effect,
                tr(
                    "Enlarge thumbnails near the center",
                    "Увеличивать миниатюры ближе к центру",
                ),
            );
        },
    );
}

fn advanced(ui: &mut Ui, config: &mut ViewerConfig, tr: &Translate) {
    ui.heading(tr("Cache and preloading", "Кеш и предзагрузка"));
    slider(
        ui,
        tr("Image cache limit", "Лимит кеша изображений"),
        Slider::new(&mut config.cache.max_ram_cache_mb, 256..=4096)
            .step_by(128.0)
            .suffix(tr(" MB", " МБ")),
    );
    ui.label(tr(
        "Restart Viewlume to apply the cache limit.",
        "Перезапустите Viewlume, чтобы применить лимит кеша.",
    ));
    ui.add_space(ui.spacing().item_spacing.y);
    slider(
        ui,
        tr(
            "Nearby images to preload",
            "Число соседних изображений для предзагрузки",
        ),
        Slider::new(&mut config.cache.prefetch_count, 0..=6),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_save_keeps_the_draft_separate_from_active_settings() {
        let mut active = ViewerConfig::default();
        let mut dialog = SettingsDialog::default();
        dialog.open(&active);
        dialog.draft.as_mut().unwrap().general.language = Language::Russian;

        assert!(!dialog.save_draft(&mut active, |_| Err("disk full".into())));
        assert_eq!(active.general.language, Language::English);
        assert_eq!(dialog.save_error.as_deref(), Some("disk full"));
        assert!(dialog.is_open());

        assert!(dialog.save_draft(&mut active, |draft| {
            assert_eq!(draft.general.language, Language::Russian);
            Ok(())
        }));
        assert_eq!(active.general.language, Language::Russian);
        assert!(dialog.save_error.is_none());
        assert!(!dialog.is_open());
    }
}
