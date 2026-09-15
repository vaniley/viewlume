use egui::{Color32, Context, Pos2, Rect, Sense, Stroke, Vec2, ViewportCommand};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::cache::ImageCache;
use crate::canvas::{render_canvas, CanvasState};
use crate::config::{DoubleClickAction, FilterMode, ViewerConfig, WindowMode};
use crate::filmstrip::{render_filmstrip, FilmstripState};
use crate::loader::decoder::DecodedImage;
use crate::loader::pipeline::{LoadResult, LoaderPipeline, Priority};
use crate::navigation::{render_edge_chevrons, FolderNavigator, NavAction};
use crate::settings_dialog::SettingsDialog;

#[derive(Clone, Copy)]
enum ToolbarIcon {
    Open,
    Fit,
    Settings,
    Window,
    Close,
}

fn icon_button(
    ui: &mut egui::Ui,
    rect: Rect,
    id: &'static str,
    icon: ToolbarIcon,
    tooltip: &'static str,
    opacity: f32,
) -> bool {
    let response = ui
        .interact(rect, ui.id().with(id), Sense::click())
        .on_hover_text(tooltip);
    let hover = ui
        .ctx()
        .animate_bool_with_time(response.id, response.hovered(), 0.14);
    let pressed = f32::from(response.is_pointer_button_down_on());
    let alpha = ((20.0 + hover * 116.0 + pressed * 38.0) * opacity) as u8;
    ui.painter().circle_filled(
        rect.center(),
        18.0 + hover * 1.5,
        Color32::from_rgba_premultiplied(20, 23, 29, alpha),
    );

    let center = rect.center();
    let color =
        Color32::from_rgba_premultiplied(238, 241, 247, ((150.0 + hover * 105.0) * opacity) as u8);
    let stroke = Stroke::new(1.8_f32, color);
    match icon {
        ToolbarIcon::Open => {
            let points = [
                Pos2::new(center.x - 9.0, center.y - 5.0),
                Pos2::new(center.x - 2.0, center.y - 5.0),
                Pos2::new(center.x + 1.0, center.y - 2.0),
                Pos2::new(center.x + 9.0, center.y - 2.0),
                Pos2::new(center.x + 7.0, center.y + 7.0),
                Pos2::new(center.x - 9.0, center.y + 7.0),
                Pos2::new(center.x - 9.0, center.y - 5.0),
            ];
            ui.painter().add(egui::Shape::line(points.to_vec(), stroke));
        }
        ToolbarIcon::Fit => {
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let corner = center + Vec2::new(sx * 8.0, sy * 8.0);
                ui.painter()
                    .line_segment([corner, corner - Vec2::new(sx * 5.0, 0.0)], stroke);
                ui.painter()
                    .line_segment([corner, corner - Vec2::new(0.0, sy * 5.0)], stroke);
            }
        }
        ToolbarIcon::Settings => {
            ui.painter().circle_stroke(center, 5.0, stroke);
            for step in 0..8 {
                let angle = step as f32 * std::f32::consts::TAU / 8.0;
                let direction = Vec2::angled(angle);
                ui.painter().line_segment(
                    [center + direction * 7.0, center + direction * 10.0],
                    stroke,
                );
            }
        }
        ToolbarIcon::Window => {
            ui.painter().rect_stroke(
                Rect::from_center_size(center, Vec2::new(17.0, 13.0)),
                2.0,
                stroke,
            );
        }
        ToolbarIcon::Close => {
            ui.painter().line_segment(
                [center + Vec2::new(-6.0, -6.0), center + Vec2::new(6.0, 6.0)],
                stroke,
            );
            ui.painter().line_segment(
                [center + Vec2::new(6.0, -6.0), center + Vec2::new(-6.0, 6.0)],
                stroke,
            );
        }
    }
    response.clicked()
}

pub struct ImageViewerApp {
    pub config: ViewerConfig,
    pub cache: Arc<ImageCache>,
    pub pipeline: LoaderPipeline,
    pub navigator: FolderNavigator,
    pub canvas_state: CanvasState,
    pub filmstrip_state: FilmstripState,
    pub settings_dialog: SettingsDialog,

    pub current_image: Option<Arc<DecodedImage>>,
    texture_dirty: bool,
    pub is_loading: bool,
    pub load_error: Option<String>,
    pub window_mode: WindowMode,

    pub last_mouse_activity: Instant,
    pub ui_opacity: f32,
    pub show_info_badge: bool,
    pub status_message: Option<(String, Instant)>,
}

impl ImageViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_target: Option<PathBuf>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(12, 14, 18);
        visuals.window_fill = Color32::from_rgb(24, 27, 33);
        visuals.extreme_bg_color = Color32::from_rgb(17, 19, 24);
        visuals.selection.bg_fill = Color32::from_rgb(37, 99, 235);
        visuals.widgets.inactive.rounding = egui::Rounding::same(7.0);
        visuals.widgets.hovered.rounding = egui::Rounding::same(7.0);
        visuals.widgets.active.rounding = egui::Rounding::same(7.0);
        cc.egui_ctx.set_visuals(visuals);
        let config = ViewerConfig::load();
        let cache = Arc::new(ImageCache::new(
            config.cache.max_ram_cache_mb,
            config.cache.thumbnail_cache_count,
        ));
        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let pipeline = LoaderPipeline::new(Arc::clone(&cache), num_cpus, cc.egui_ctx.clone());

        let mut app = Self {
            window_mode: config.window.mode,
            config,
            cache,
            pipeline,
            navigator: FolderNavigator::default(),
            canvas_state: CanvasState::default(),
            filmstrip_state: FilmstripState::default(),
            settings_dialog: SettingsDialog::default(),

            current_image: None,
            texture_dirty: false,
            is_loading: false,
            load_error: None,

            last_mouse_activity: Instant::now(),
            ui_opacity: 1.0,
            show_info_badge: true,
            status_message: None,
        };

        if let Some(target) = initial_target {
            app.open_target(&target);
        }

        app
    }

    pub fn open_target(&mut self, path: &Path) {
        self.navigator.scan_directory(path);
        self.filmstrip_state.clear();
        self.load_current_image(true);
    }

    pub fn load_current_image(&mut self, reset_view: bool) {
        if let Some(path) = self.navigator.current_path().cloned() {
            self.pipeline.next_epoch();
            self.load_error = None;

            if reset_view {
                self.canvas_state
                    .prepare_image_change(self.current_image.as_deref());
            }

            // Check if already in cache
            if let Some(cached) = self.cache.get_full(&path) {
                self.current_image = Some(Arc::clone(&cached));
                self.texture_dirty = true;
                self.is_loading = false;
                self.prefetch_adjacent();
            } else {
                self.is_loading = true;
                // Request immediate load
                self.pipeline.request_full_image(
                    path.clone(),
                    self.navigator.current_index,
                    Priority::Immediate,
                );
            }
        } else {
            self.current_image = None;
            self.is_loading = false;
        }
    }

    pub fn prefetch_adjacent(&self) {
        let total = self.navigator.total_count();
        if total <= 1 || self.config.cache.prefetch_count == 0 {
            return;
        }

        let cur = self.navigator.current_index;
        let count = self.config.cache.prefetch_count;
        let wrap = self.config.navigation.loop_folder;

        for offset in 1..=count {
            // Forward
            let fwd_idx = if cur + offset < total {
                Some(cur + offset)
            } else if wrap {
                Some((cur + offset) % total)
            } else {
                None
            };
            if let Some(idx) = fwd_idx {
                if let Some(path) = self.navigator.files.get(idx) {
                    self.pipeline
                        .request_full_image(path.clone(), idx, Priority::Prefetch);
                }
            }

            // Backward
            let bwd_idx = if cur >= offset {
                Some(cur - offset)
            } else if wrap {
                Some(total - (offset - cur))
            } else {
                None
            };
            if let Some(idx) = bwd_idx {
                if let Some(path) = self.navigator.files.get(idx) {
                    self.pipeline
                        .request_full_image(path.clone(), idx, Priority::Prefetch);
                }
            }
        }
    }

    pub fn navigate_next(&mut self) {
        if self
            .navigator
            .next(self.config.navigation.loop_folder)
            .is_some()
        {
            self.canvas_state.set_transition_direction(1.0);
            self.load_current_image(true);
            self.reset_activity();
        }
    }

    pub fn navigate_prev(&mut self) {
        if self
            .navigator
            .prev(self.config.navigation.loop_folder)
            .is_some()
        {
            self.canvas_state.set_transition_direction(-1.0);
            self.load_current_image(true);
            self.reset_activity();
        }
    }

    pub fn jump_to(&mut self, index: usize) {
        let direction = if index < self.navigator.current_index {
            -1.0
        } else {
            1.0
        };
        if self.navigator.jump_to(index).is_some() {
            self.canvas_state.set_transition_direction(direction);
            self.load_current_image(true);
            self.reset_activity();
        }
    }

    pub fn reset_activity(&mut self) {
        self.last_mouse_activity = Instant::now();
        self.ui_opacity = 1.0;
    }

    pub fn show_toast(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), Instant::now()));
    }

    pub fn toggle_window_mode(&mut self, ctx: &Context) {
        match self.window_mode {
            WindowMode::Overlay => {
                self.window_mode = WindowMode::Windowed;
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
                ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
                self.show_toast("Mode: Windowed");
            }
            WindowMode::Windowed => {
                self.window_mode = WindowMode::Overlay;
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
                ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
                self.show_toast("Mode: Fullscreen overlay");
            }
        }
    }

    pub fn process_incoming_results(&mut self, ctx: &Context) {
        while let Some(res) = self.pipeline.try_recv_result() {
            match res {
                LoadResult::FullImage {
                    path,
                    index,
                    epoch,
                    result,
                } => {
                    let cur_idx = self.navigator.current_index;
                    let cur_path = self.navigator.current_path();

                    // Only update active image if it matches current index & path & epoch
                    if epoch == self.pipeline.current_epoch()
                        && Some(&path) == cur_path
                        && index == cur_idx
                    {
                        self.is_loading = false;
                        match result {
                            Ok(img) => {
                                self.canvas_state.update_texture(
                                    ctx,
                                    &img,
                                    self.config.rendering.filter_mode,
                                    true,
                                    self.config.rendering.animate_image_transitions,
                                );
                                self.current_image = Some(img);
                                self.texture_dirty = false;
                                self.load_error = None;
                                self.prefetch_adjacent();
                            }
                            Err(e) => {
                                self.load_error = Some(e);
                                self.current_image = None;
                            }
                        }
                        ctx.request_repaint();
                    }
                }
                LoadResult::Thumbnail {
                    path,
                    index,
                    result,
                } => {
                    if self.navigator.files.get(index) == Some(&path) {
                        if let Ok(thumb) = result {
                            self.filmstrip_state
                                .insert_loaded_thumbnail(ctx, path, index, thumb);
                            ctx.request_repaint();
                        }
                    }
                }
            }
        }
    }

    pub fn handle_keyboard_inputs(&mut self, ctx: &Context) {
        let (
            left_pressed,
            right_pressed,
            home_pressed,
            end_pressed,
            f_pressed,
            zero_pressed,
            one_pressed,
            f11_pressed,
            esc_pressed,
            o_pressed,
            s_pressed,
            t_pressed,
            n_pressed,
            i_pressed,
        ) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::A),
                i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::D),
                i.key_pressed(egui::Key::Home),
                i.key_pressed(egui::Key::End),
                i.key_pressed(egui::Key::F),
                i.key_pressed(egui::Key::Num0),
                i.key_pressed(egui::Key::Num1),
                i.key_pressed(egui::Key::F11),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::O),
                i.key_pressed(egui::Key::S),
                i.key_pressed(egui::Key::T),
                i.key_pressed(egui::Key::N),
                i.key_pressed(egui::Key::I),
            )
        });

        if left_pressed {
            self.navigate_prev();
        }
        if right_pressed {
            self.navigate_next();
        }
        if home_pressed && !self.navigator.files.is_empty() {
            self.jump_to(0);
        }
        if end_pressed && !self.navigator.files.is_empty() {
            let last = self.navigator.files.len() - 1;
            self.jump_to(last);
        }
        if f_pressed || zero_pressed {
            if let Some(ref img) = self.current_image {
                let vp = ctx.screen_rect();
                self.canvas_state
                    .reset_view_fit(img.width as f32, img.height as f32, vp);
                self.show_toast("View: Fit to Window");
            }
        }
        if one_pressed {
            if let Some(ref img) = self.current_image {
                let vp = ctx.screen_rect();
                self.canvas_state
                    .reset_view_actual_size(img.width as f32, img.height as f32, vp);
                self.show_toast("View: 1:1 (100%)");
            }
        }
        if f11_pressed {
            self.toggle_window_mode(ctx);
        }
        if esc_pressed {
            if self.settings_dialog.is_open {
                self.settings_dialog.is_open = false;
            } else if self.window_mode == WindowMode::Overlay {
                self.toggle_window_mode(ctx);
            }
        }
        if o_pressed {
            if let Some(file) = rfd::FileDialog::new()
                .add_filter(
                    "Images",
                    &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tiff", "qoi"],
                )
                .pick_file()
            {
                self.open_target(&file);
            }
        }
        if s_pressed {
            self.settings_dialog.is_open = !self.settings_dialog.is_open;
        }
        if t_pressed {
            use crate::config::FilmstripVisibility;
            self.config.filmstrip.visibility = match self.config.filmstrip.visibility {
                FilmstripVisibility::Hover => FilmstripVisibility::Always,
                FilmstripVisibility::Always => FilmstripVisibility::Hidden,
                FilmstripVisibility::Hidden => FilmstripVisibility::Hover,
            };
            let label = match self.config.filmstrip.visibility {
                FilmstripVisibility::Hover => "Filmstrip: Auto-hover",
                FilmstripVisibility::Always => "Filmstrip: Always visible",
                FilmstripVisibility::Hidden => "Filmstrip: Hidden",
            };
            self.show_toast(label);
        }
        if n_pressed {
            self.config.rendering.filter_mode = match self.config.rendering.filter_mode {
                FilterMode::Auto => FilterMode::Nearest,
                FilterMode::Nearest => FilterMode::Bilinear,
                FilterMode::Bilinear => FilterMode::Auto,
            };
            let label = match self.config.rendering.filter_mode {
                FilterMode::Auto => "Filter: Auto (Crisp zoom)",
                FilterMode::Nearest => "Filter: Nearest (Pixel art)",
                FilterMode::Bilinear => "Filter: Bilinear (Smooth)",
            };
            if let Some(ref img) = self.current_image {
                self.canvas_state.update_texture(
                    ctx,
                    img,
                    self.config.rendering.filter_mode,
                    false,
                    false,
                );
            }
            self.show_toast(label);
        }
        if i_pressed {
            self.show_info_badge = !self.show_info_badge;
        }
    }

    pub fn handle_drag_and_drop(&mut self, ctx: &Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            if let Some(path) = file.path {
                self.open_target(&path);
                break;
            }
        }
    }

    pub fn update_ui_fade(&mut self, ctx: &Context, cursor_pos: Option<Pos2>, dt: f32) {
        let mouse_moved =
            ctx.input(|i| i.pointer.delta() != Vec2::ZERO || i.raw_scroll_delta != Vec2::ZERO);
        if mouse_moved || self.canvas_state.is_dragging {
            self.last_mouse_activity = Instant::now();
        }

        let elapsed = self.last_mouse_activity.elapsed().as_secs_f32();
        let target_opacity = if !self.config.window.auto_hide_ui
            || elapsed < self.config.window.ui_fade_timeout_secs
        {
            1.0_f32
        } else {
            // Keep visible if cursor is hovering near top or bottom or edges
            let screen = ctx.screen_rect();
            let in_active_zone = cursor_pos.is_some_and(|pos| {
                pos.y < screen.min.y + 60.0
                    || pos.y > screen.max.y - 100.0
                    || pos.x < screen.min.x + 80.0
                    || pos.x > screen.max.x - 80.0
            });
            if in_active_zone {
                1.0_f32
            } else {
                0.0_f32
            }
        };

        let fade_speed = 4.0_f32;
        self.ui_opacity += (target_opacity - self.ui_opacity) * (fade_speed * dt).min(1.0_f32);
        if (self.ui_opacity - target_opacity).abs() > 0.01 {
            ctx.request_repaint();
        }
    }
}

impl eframe::App for ImageViewerApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.window_mode == WindowMode::Overlay {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            let color = self.config.window.bg_color;
            [
                color[0] as f32 / 255.0,
                color[1] as f32 / 255.0,
                color[2] as f32 / 255.0,
                1.0,
            ]
        }
    }

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.unstable_dt).max(0.001);
        let cursor_pos = ctx.input(|i| i.pointer.hover_pos());

        // Process async worker responses
        self.process_incoming_results(ctx);
        if self.texture_dirty {
            if let Some(image) = self.current_image.as_ref() {
                self.canvas_state.update_texture(
                    ctx,
                    image,
                    self.config.rendering.filter_mode,
                    true,
                    self.config.rendering.animate_image_transitions,
                );
            }
            self.texture_dirty = false;
        }

        // Handle hotkeys & drag-and-drop
        self.handle_keyboard_inputs(ctx);
        self.handle_drag_and_drop(ctx);

        // Manage UI auto-fade
        self.update_ui_fade(ctx, cursor_pos, dt);

        // Draw background
        let bg_color = if self.window_mode == WindowMode::Overlay {
            Color32::TRANSPARENT
        } else {
            Color32::from_rgb(
                self.config.window.bg_color[0],
                self.config.window.bg_color[1],
                self.config.window.bg_color[2],
            )
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg_color))
            .show(ctx, |ui| {
                let viewport = ui.available_rect_before_wrap();
                let filmstrip_height = self.config.filmstrip.thumbnail_size as f32 + 48.0;
                let pointer_over_filmstrip = self.config.filmstrip.visibility
                    != crate::config::FilmstripVisibility::Hidden
                    && cursor_pos.is_some_and(|position| {
                        position.y
                            >= viewport.max.y
                                - self
                                    .config
                                    .filmstrip
                                    .bottom_hover_height
                                    .max(filmstrip_height)
                    });

                // 1. Render Interactive Canvas (Image, Pan, Zoom)
                let canvas_resp = render_canvas(
                    ui,
                    &mut self.canvas_state,
                    self.current_image.as_ref(),
                    &self.config.rendering,
                    self.config.window.show_checkerboard_for_transparent,
                    !pointer_over_filmstrip,
                );

                // Double-click on canvas action
                if canvas_resp.double_clicked {
                    match self.config.window.double_click_action {
                        DoubleClickAction::ToggleWindowMode => {
                            self.toggle_window_mode(ctx);
                        }
                        DoubleClickAction::ToggleFitToWindow => {
                            if let Some(ref img) = self.current_image {
                                self.canvas_state.reset_view_fit(
                                    img.width as f32,
                                    img.height as f32,
                                    viewport,
                                );
                            }
                        }
                        DoubleClickAction::ToggleActualSize => {
                            if let Some(ref img) = self.current_image {
                                self.canvas_state.reset_view_actual_size(
                                    img.width as f32,
                                    img.height as f32,
                                    viewport,
                                );
                            }
                        }
                        DoubleClickAction::None => {}
                    }
                }

                // 2. Render Screen Edge Chevrons (< and >)
                let nav_action = render_edge_chevrons(
                    ui,
                    viewport,
                    &self.config.navigation,
                    self.navigator.total_count(),
                    self.ui_opacity,
                );
                match nav_action {
                    NavAction::Prev => self.navigate_prev(),
                    NavAction::Next => self.navigate_next(),
                    NavAction::None => {}
                }

                // 3. Render Bottom Filmstrip Gallery
                let film_resp = render_filmstrip(
                    ui,
                    &mut self.filmstrip_state,
                    &self.pipeline,
                    &self.navigator.files,
                    self.navigator.current_index,
                    &self.config.filmstrip,
                    viewport,
                    cursor_pos,
                    dt,
                );
                if let Some(idx) = film_resp.selected_index {
                    self.jump_to(idx);
                }

                // 4. Frameless icon controls leave the image as the only visual focus.
                if self.ui_opacity > 0.05 {
                    let button_size = Vec2::splat(40.0);
                    let top = viewport.min.y + 12.0;
                    let open_rect =
                        Rect::from_min_size(Pos2::new(viewport.min.x + 12.0, top), button_size);
                    let fit_rect = open_rect.translate(Vec2::new(44.0, 0.0));
                    if icon_button(
                        ui,
                        open_rect,
                        "open",
                        ToolbarIcon::Open,
                        "Открыть изображение (O)",
                        self.ui_opacity,
                    ) {
                        if let Some(file) = rfd::FileDialog::new()
                            .add_filter(
                                "Images",
                                &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tiff", "qoi"],
                            )
                            .pick_file()
                        {
                            self.open_target(&file);
                        }
                    }
                    if icon_button(
                        ui,
                        fit_rect,
                        "fit",
                        ToolbarIcon::Fit,
                        "Вписать в окно (F)",
                        self.ui_opacity,
                    ) {
                        if let Some(ref img) = self.current_image {
                            self.canvas_state.reset_view_fit(
                                img.width as f32,
                                img.height as f32,
                                viewport,
                            );
                        }
                    }

                    let close_rect =
                        Rect::from_min_size(Pos2::new(viewport.max.x - 52.0, top), button_size);
                    let window_rect = close_rect.translate(Vec2::new(-44.0, 0.0));
                    let settings_rect = close_rect.translate(Vec2::new(-88.0, 0.0));
                    if icon_button(
                        ui,
                        settings_rect,
                        "settings",
                        ToolbarIcon::Settings,
                        "Настройки (S)",
                        self.ui_opacity,
                    ) {
                        self.settings_dialog.is_open = true;
                    }
                    if icon_button(
                        ui,
                        window_rect,
                        "window-mode",
                        ToolbarIcon::Window,
                        "Переключить режим окна (F11)",
                        self.ui_opacity,
                    ) {
                        self.toggle_window_mode(ctx);
                    }
                    if self.window_mode == WindowMode::Overlay
                        && icon_button(
                            ui,
                            close_rect,
                            "close",
                            ToolbarIcon::Close,
                            "Закрыть",
                            self.ui_opacity,
                        )
                    {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                }

                // 5. Render Info Badge in bottom-left
                if self.show_info_badge && self.ui_opacity > 0.05 {
                    if let Some(ref img) = self.current_image {
                        let badge_alpha = (210.0 * self.ui_opacity) as u8;
                        let zoom_pct = (self.canvas_state.scale * 100.0).round() as i64;
                        let filter_name = match self.config.rendering.filter_mode {
                            FilterMode::Auto => "Auto",
                            FilterMode::Nearest => "Nearest",
                            FilterMode::Bilinear => "Bilinear",
                        };

                        let info_text = format!(
                            "{}×{} • {}% • {}",
                            img.width, img.height, zoom_pct, filter_name
                        );
                        let badge_rect = Rect::from_min_size(
                            Pos2::new(viewport.min.x + 20.0, viewport.max.y - 40.0),
                            Vec2::new(200.0, 26.0),
                        );

                        ui.painter().rect(
                            badge_rect,
                            6.0_f32,
                            Color32::from_rgba_premultiplied(18, 18, 22, badge_alpha),
                            Stroke::NONE,
                        );

                        ui.painter().text(
                            badge_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            info_text,
                            egui::FontId::proportional(12.0),
                            Color32::from_rgba_premultiplied(220, 220, 230, badge_alpha),
                        );
                    }
                }

                // 6. Loading Spinner or Error Banner
                if self.is_loading {
                    let spinner_rect = Rect::from_center_size(viewport.center(), Vec2::splat(40.0));
                    ui.allocate_rect(spinner_rect, egui::Sense::hover());
                    ui.painter().text(
                        viewport.center(),
                        egui::Align2::CENTER_CENTER,
                        "Loading...",
                        egui::FontId::proportional(16.0),
                        Color32::WHITE,
                    );
                } else if let Some(ref err) = self.load_error {
                    ui.painter().text(
                        viewport.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("Failed to load image:\n{}", err),
                        egui::FontId::proportional(16.0),
                        Color32::from_rgb(239, 68, 68),
                    );
                }

                // 7. Toast notification
                if let Some((ref msg, time)) = self.status_message {
                    let elapsed = time.elapsed().as_secs_f32();
                    if elapsed < 2.0 {
                        let toast_alpha = ((1.0 - (elapsed / 2.0).powi(2)) * 240.0) as u8;
                        let toast_pos = Pos2::new(viewport.center().x, viewport.min.y + 64.0);
                        let text_rect = Rect::from_center_size(toast_pos, Vec2::new(280.0, 32.0));

                        ui.painter().rect(
                            text_rect,
                            8.0_f32,
                            Color32::from_rgba_premultiplied(24, 24, 28, toast_alpha),
                            Stroke::new(
                                1.0_f32,
                                Color32::from_rgba_premultiplied(255, 255, 255, toast_alpha / 4),
                            ),
                        );
                        ui.painter().text(
                            toast_pos,
                            egui::Align2::CENTER_CENTER,
                            msg,
                            egui::FontId::proportional(13.0),
                            Color32::from_rgba_premultiplied(255, 255, 255, toast_alpha),
                        );
                        ctx.request_repaint();
                    } else {
                        self.status_message = None;
                    }
                }
            });

        // 8. Settings Dialog
        if self.settings_dialog.show(ctx, &mut self.config) {
            // Re-apply settings if changed
            if let Some(ref img) = self.current_image {
                self.canvas_state.update_texture(
                    ctx,
                    img,
                    self.config.rendering.filter_mode,
                    false,
                    false,
                );
            }
        }
    }
}
