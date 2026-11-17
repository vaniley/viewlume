use crossbeam_channel::{unbounded, Receiver, Sender};
use egui::{Color32, Context, Pos2, Rect, Sense, Stroke, Vec2, ViewportCommand};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::cache::ImageCache;
use crate::canvas::{render_canvas, CanvasInput, CanvasState, TextureSizeError};
use crate::config::{DoubleClickAction, FilterMode, ViewerConfig, WindowMode};
use crate::filmstrip::{render_filmstrip, FilmstripState, ScrollNav};
use crate::loader::decoder::DecodedImage;
use crate::loader::pipeline::{LoadResult, LoaderPipeline, Priority};
use crate::navigation::{
    render_edge_chevrons, FolderNavigator, FolderScan, NavAction, SUPPORTED_IMAGE_EXTENSIONS,
};
use crate::presentation::PresentationPolicy;
use crate::settings_dialog::{SettingsChanges, SettingsDialog};

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
        Color32::from_rgba_unmultiplied(20, 23, 29, alpha),
    );

    let center = rect.center();
    let color =
        Color32::from_rgba_unmultiplied(238, 241, 247, ((150.0 + hover * 105.0) * opacity) as u8);
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

struct ScanResult {
    generation: u64,
    scan: FolderScan,
}

#[derive(Default)]
struct NavigationPace {
    last_navigation: Option<Instant>,
    direction: i8,
    streak: usize,
}

impl NavigationPace {
    fn record(&mut self, direction: i8) {
        self.record_at(direction, Instant::now());
    }

    fn record_at(&mut self, direction: i8, now: Instant) {
        let interval = self
            .last_navigation
            .and_then(|previous| now.checked_duration_since(previous));
        self.streak = match (self.direction == direction, interval) {
            (true, Some(duration)) if duration <= Duration::from_millis(160) => {
                (self.streak + 2).min(6)
            }
            (true, Some(duration)) if duration <= Duration::from_millis(360) => {
                (self.streak + 1).min(6)
            }
            _ => 0,
        };
        self.direction = direction;
        self.last_navigation = Some(now);
    }

    fn prefetch_counts(&self, base: usize) -> (usize, usize) {
        match self.direction {
            1 if self.streak > 0 => (base + self.streak, base.min(1)),
            -1 if self.streak > 0 => (base.min(1), base + self.streak),
            _ => (base, base),
        }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
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
    current_image_path: Option<PathBuf>,
    current_image_is_preview: bool,
    texture_dirty: bool,
    texture_reset_view: bool,
    texture_animate: bool,
    presentation: PresentationPolicy,
    ignore_canvas_double_click_until: f64,
    settings_held_keys: HashSet<egui::Key>,
    pub is_loading: bool,
    pub load_error: Option<String>,
    pub window_mode: WindowMode,

    pub last_mouse_activity: Instant,
    pub ui_opacity: f32,
    pub show_info_badge: bool,
    pub status_message: Option<(String, Instant)>,

    scan_result_tx: Sender<ScanResult>,
    scan_result_rx: Receiver<ScanResult>,
    scan_generation: u64,
    pending_open_path: Option<PathBuf>,
    queued_open_target: Option<PathBuf>,
    navigation_pace: NavigationPace,
    pending_load_results: VecDeque<LoadResult>,
    animation_frame_index: usize,
    animation_deadline: Instant,
    animation_paused: bool,
    animation_speed: f32,
}

impl ImageViewerApp {
    fn tr(&self, english: &'static str, russian: &'static str) -> &'static str {
        if self.config.general.language.is_russian() {
            russian
        } else {
            english
        }
    }

    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_target: Option<PathBuf>,
        config: ViewerConfig,
        presentation: PresentationPolicy,
    ) -> Self {
        Self::with_context(&cc.egui_ctx, initial_target, config, presentation)
    }

    fn with_context(
        ctx: &Context,
        initial_target: Option<PathBuf>,
        config: ViewerConfig,
        presentation: PresentationPolicy,
    ) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(12, 14, 18);
        visuals.window_fill = Color32::from_rgb(24, 27, 33);
        visuals.extreme_bg_color = Color32::from_rgb(17, 19, 24);
        visuals.selection.bg_fill = Color32::from_rgb(37, 99, 235);
        visuals.widgets.inactive.rounding = egui::Rounding::same(7.0);
        visuals.widgets.hovered.rounding = egui::Rounding::same(7.0);
        visuals.widgets.active.rounding = egui::Rounding::same(7.0);
        ctx.set_visuals(visuals);
        let cache = Arc::new(ImageCache::new(
            config.cache.max_ram_cache_mb,
            config.cache.thumbnail_cache_count,
        ));
        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let pipeline = LoaderPipeline::new(
            Arc::clone(&cache),
            num_cpus,
            ctx.clone(),
            config.filmstrip.thumbnail_size,
        );
        let (scan_result_tx, scan_result_rx) = unbounded();

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
            current_image_path: None,
            current_image_is_preview: false,
            texture_dirty: false,
            texture_reset_view: true,
            texture_animate: false,
            presentation,
            ignore_canvas_double_click_until: f64::NEG_INFINITY,
            settings_held_keys: HashSet::new(),
            is_loading: false,
            load_error: None,

            last_mouse_activity: Instant::now(),
            ui_opacity: 1.0,
            show_info_badge: true,
            status_message: None,

            scan_result_tx,
            scan_result_rx,
            scan_generation: 0,
            pending_open_path: None,
            queued_open_target: None,
            navigation_pace: NavigationPace::default(),
            pending_load_results: VecDeque::new(),
            animation_frame_index: 0,
            animation_deadline: Instant::now(),
            animation_paused: false,
            animation_speed: 1.0,
        };

        if let Some(target) = initial_target {
            app.open_target(&target, ctx);
        }

        app
    }

    pub fn open_target(&mut self, path: &Path, ctx: &Context) {
        // Open actions can originate after filmstrip meshes have been painted.
        // Keep their texture handles alive until this frame has been submitted.
        self.queued_open_target = Some(path.to_path_buf());
        ctx.request_repaint();
    }

    fn process_queued_open(&mut self, ctx: &Context) {
        if let Some(path) = self.queued_open_target.take() {
            self.open_target_now(&path, ctx);
        }
    }

    fn open_target_now(&mut self, path: &Path, ctx: &Context) {
        let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.scan_generation = self.scan_generation.wrapping_add(1);
        let generation = self.scan_generation;
        let sender = self.scan_result_tx.clone();
        let repaint = ctx.clone();
        let scan_target = target.clone();
        let _ = thread::Builder::new()
            .name("folder-scan".to_string())
            .spawn(move || {
                let scan = FolderNavigator::scan(&scan_target);
                let _ = sender.send(ScanResult { generation, scan });
                repaint.request_repaint();
            });

        self.navigation_pace.reset();
        self.pipeline.next_epoch();
        self.load_error = None;
        self.is_loading = true;
        self.navigator = FolderNavigator::default();
        self.filmstrip_state.clear();

        // Decode an explicitly opened file immediately, in parallel with its folder scan.
        if target.is_file() && FolderNavigator::is_supported_image(&target) {
            self.canvas_state
                .prepare_image_change(self.current_image.as_deref());
            self.is_loading = true;
            self.pending_open_path = Some(target.clone());
            let priority = if self.current_image.is_none() {
                Priority::ImmediateWithPreview
            } else {
                Priority::Immediate
            };
            if let Some(cached) =
                self.pipeline
                    .request_full_image(target.clone(), usize::MAX, priority)
            {
                self.activate_image(target, cached);
            }
        } else {
            self.pending_open_path = None;
        }
    }

    fn process_scan_results(&mut self) {
        while let Ok(result) = self.scan_result_rx.try_recv() {
            if result.generation != self.scan_generation {
                continue;
            }
            self.navigator.apply_scan(result.scan);
            self.filmstrip_state.clear();

            let current_path = self.navigator.current_path();
            if current_path.is_some() && current_path == self.pending_open_path.as_ref() {
                // The explicitly opened file is already decoding in parallel.
            } else if current_path.is_some() && current_path == self.current_image_path.as_ref() {
                self.is_loading = false;
                self.prefetch_adjacent();
            } else {
                self.load_current_image(true);
            }
        }
    }

    pub fn load_current_image(&mut self, reset_view: bool) {
        if let Some(path) = self.navigator.current_path().cloned() {
            self.pipeline.next_epoch();
            self.pending_open_path = None;
            self.load_error = None;

            if reset_view {
                self.canvas_state
                    .prepare_image_change(self.current_image.as_deref());
            }

            // Check if already in cache
            if let Some(cached) = self.cache.get_full(&path) {
                self.activate_image(path, cached);
            } else {
                self.is_loading = true;
                let priority = if self.current_image.is_none() {
                    Priority::ImmediateWithPreview
                } else {
                    Priority::Immediate
                };
                // Request immediate load
                if let Some(cached) = self.pipeline.request_full_image(
                    path.clone(),
                    self.navigator.current_index,
                    priority,
                ) {
                    self.activate_image(path, cached);
                }
            }
        } else {
            self.pending_open_path = None;
            self.current_image = None;
            self.current_image_path = None;
            self.current_image_is_preview = false;
            self.is_loading = false;
        }
    }

    fn activate_image(&mut self, path: PathBuf, image: Arc<DecodedImage>) {
        let replacing_preview =
            self.current_image_is_preview && self.current_image_path.as_ref() == Some(&path);
        let delay = image
            .animation_frames
            .first()
            .map(|frame| frame.delay)
            .unwrap_or_default();
        self.current_image = Some(image);
        self.current_image_path = Some(path);
        self.current_image_is_preview = false;
        self.texture_dirty = true;
        self.texture_reset_view = !replacing_preview;
        self.texture_animate =
            !replacing_preview && self.config.rendering.animate_image_transitions;
        self.animation_frame_index = 0;
        self.animation_deadline = Instant::now() + delay;
        self.animation_paused = false;
        self.is_loading = false;
        self.load_error = None;
        self.pending_open_path = None;
        self.prefetch_adjacent();
    }

    fn image_upload_failed(&mut self, error: TextureSizeError) {
        let TextureSizeError::TooLarge {
            width,
            height,
            max_texture_side,
        } = error;
        self.load_error = Some(if self.config.general.language.is_russian() {
            format!("Размер текстуры {width}×{height} превышает ограничение видеокарты {max_texture_side}×{max_texture_side}.")
        } else {
            format!("Image texture {width}×{height} exceeds this GPU's limit of {max_texture_side}×{max_texture_side}.")
        });
        self.current_image = None;
        self.current_image_is_preview = false;
        self.texture_dirty = false;
        self.is_loading = false;
        self.pending_open_path = None;
        // Do not free a texture in the same frame as a filter/preview upload.
        // egui-wgpu 0.29 destroys freed textures before submitting queued writes.
        // Keep the bounded canvas assets; no image is painted while this is None.
        self.canvas_state.initialized = false;
        self.canvas_state.is_dragging = false;
    }

    fn upload_pending_texture(&mut self, ctx: &Context) {
        if !self.texture_dirty {
            return;
        }
        self.texture_dirty = false;
        if let Some(image) = self.current_image.as_ref() {
            if let Err(error) = self.canvas_state.update_texture(
                ctx,
                image,
                self.config.rendering.filter_mode,
                self.texture_reset_view,
                self.texture_animate,
            ) {
                self.image_upload_failed(error);
            }
        }
    }

    pub fn prefetch_adjacent(&self) {
        let total = self.navigator.total_count();
        if total <= 1 || self.config.cache.prefetch_count == 0 {
            return;
        }

        let cur = self.navigator.current_index;
        let base_count = self.config.cache.prefetch_count.min(total - 1);
        let (forward_count, backward_count) = self.navigation_pace.prefetch_counts(base_count);
        let wrap = self.config.navigation.loop_folder;

        let mut seen = Vec::with_capacity(forward_count + backward_count);

        for offset in 1..=forward_count.max(backward_count) {
            // Forward
            let fwd_idx = if offset > forward_count {
                None
            } else if cur + offset < total {
                Some(cur + offset)
            } else if wrap {
                Some((cur + offset) % total)
            } else {
                None
            };
            if let Some(idx) = fwd_idx {
                if idx != cur && !seen.contains(&idx) {
                    seen.push(idx);
                    if let Some(path) = self.navigator.files.get(idx) {
                        self.pipeline
                            .request_full_image(path.clone(), idx, Priority::Prefetch);
                    }
                }
            }

            // Backward — use wrapping subtraction to avoid underflow
            let bwd_idx = if offset > backward_count {
                None
            } else if cur >= offset {
                Some(cur - offset)
            } else if wrap {
                Some(total - offset + cur)
            } else {
                None
            };
            if let Some(idx) = bwd_idx {
                if idx != cur && !seen.contains(&idx) {
                    seen.push(idx);
                    if let Some(path) = self.navigator.files.get(idx) {
                        self.pipeline
                            .request_full_image(path.clone(), idx, Priority::Prefetch);
                    }
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
            self.navigation_pace.record(1);
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
            self.navigation_pace.record(-1);
            self.canvas_state.set_transition_direction(-1.0);
            self.load_current_image(true);
            self.reset_activity();
        }
    }

    pub fn jump_to(&mut self, index: usize) {
        if index == self.navigator.current_index {
            return;
        }
        let direction = if index < self.navigator.current_index {
            -1.0
        } else {
            1.0
        };
        if self.navigator.jump_to(index).is_some() {
            self.navigation_pace.record(direction as i8);
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
                ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
                self.show_toast(self.tr("Mode: Windowed", "Режим: оконный"));
            }
            WindowMode::Windowed => {
                self.window_mode = WindowMode::Overlay;
                ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
                self.show_toast(
                    self.tr("Mode: Fullscreen overlay", "Режим: полноэкранный оверлей"),
                );
            }
        }
    }

    pub fn process_incoming_results(&mut self, ctx: &Context) {
        const MAX_THUMB_UPLOADS_PER_FRAME: usize = 8;
        let mut thumb_uploads = 0;

        while let Some(res) = self
            .pending_load_results
            .pop_front()
            .or_else(|| self.pipeline.try_recv_result())
        {
            match res {
                LoadResult::Preview {
                    path,
                    index,
                    epoch,
                    width,
                    height,
                    image,
                } => {
                    let matches_navigator = self.navigator.current_path() == Some(&path)
                        && self.navigator.current_index == index;
                    let matches_pending = self.pending_open_path.as_ref() == Some(&path);
                    if epoch == self.pipeline.current_epoch()
                        && (matches_navigator || matches_pending)
                        && self.current_image.is_none()
                    {
                        let preview = Arc::new(DecodedImage {
                            width,
                            height,
                            bytes_size: image.as_raw().len() * std::mem::size_of::<Color32>(),
                            color_image: image,
                            animation_frames: Arc::from([]),
                        });
                        self.current_image = Some(preview);
                        self.current_image_path = Some(path);
                        self.current_image_is_preview = true;
                        // Coalesce queued preview/full results before any GPU upload.
                        self.texture_dirty = true;
                        self.texture_reset_view = true;
                        self.texture_animate = false;
                        ctx.request_repaint();
                    }
                }
                LoadResult::FullImage {
                    path,
                    index,
                    epoch,
                    result,
                } => {
                    let cur_idx = self.navigator.current_index;
                    let cur_path = self.navigator.current_path();

                    let matches_navigator = Some(&path) == cur_path && index == cur_idx;
                    let matches_pending = self.pending_open_path.as_ref() == Some(&path);

                    // Only update active image if it still matches the latest open/navigation.
                    if epoch == self.pipeline.current_epoch()
                        && (matches_navigator || matches_pending)
                    {
                        self.is_loading = false;
                        match result {
                            Ok(img) => {
                                self.activate_image(path, img);
                            }
                            Err(e) => {
                                self.load_error = Some(e);
                                self.current_image = None;
                                self.current_image_path = Some(path.clone());
                                self.current_image_is_preview = false;
                                if matches_pending {
                                    self.pending_open_path = None;
                                }
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
                    if thumb_uploads >= MAX_THUMB_UPLOADS_PER_FRAME {
                        self.pending_load_results.push_front(LoadResult::Thumbnail {
                            path,
                            index,
                            result,
                        });
                        ctx.request_repaint();
                        break;
                    }
                    if self.navigator.files.get(index) == Some(&path) {
                        if let Ok(thumb) = result {
                            self.filmstrip_state
                                .insert_loaded_thumbnail(ctx, path, index, thumb);
                            thumb_uploads += 1;
                            ctx.request_repaint();
                        }
                    }
                }
            }
        }
    }

    pub fn handle_keyboard_inputs(&mut self, ctx: &Context) {
        if self.settings_dialog.is_open {
            return;
        }
        // A key held in Settings remains owned by Settings until a fresh press.
        ctx.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: true,
                    ..
                } => !self.settings_held_keys.contains(key),
                egui::Event::Key { key, .. } => {
                    self.settings_held_keys.remove(key);
                    true
                }
                _ => true,
            });
        });
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
                i.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            pressed: true,
                            repeat: false,
                            ..
                        }
                    )
                }),
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
                self.show_toast(self.tr("View: Fit to window", "Вид: вписано в окно"));
            }
        }
        if one_pressed {
            if let Some(ref img) = self.current_image {
                let vp = ctx.screen_rect();
                self.canvas_state
                    .reset_view_actual_size(img.width as f32, img.height as f32, vp);
                self.show_toast(self.tr("View: 1:1 (100%)", "Вид: 1:1 (100%)"));
            }
        }
        if f11_pressed {
            self.toggle_window_mode(ctx);
        }
        if esc_pressed && self.window_mode == WindowMode::Overlay {
            self.toggle_window_mode(ctx);
        }
        if o_pressed {
            if let Some(file) = rfd::FileDialog::new()
                .add_filter("Images", SUPPORTED_IMAGE_EXTENSIONS)
                .pick_file()
            {
                self.open_target(&file, ctx);
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
                FilmstripVisibility::Hover => {
                    self.tr("Filmstrip: On hover", "Карусель: при наведении")
                }
                FilmstripVisibility::Always => self.tr("Filmstrip: Always", "Карусель: всегда"),
                FilmstripVisibility::Hidden => self.tr("Filmstrip: Hidden", "Карусель: скрыта"),
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
                FilterMode::Auto => self.tr("Filter: Auto", "Фильтр: авто"),
                FilterMode::Nearest => self.tr("Filter: Nearest", "Фильтр: без сглаживания"),
                FilterMode::Bilinear => self.tr("Filter: Bilinear", "Фильтр: билинейный"),
            };
            if let Err(error) = self
                .canvas_state
                .set_filter_mode(ctx, self.config.rendering.filter_mode)
            {
                self.image_upload_failed(error);
            }
            self.show_toast(label);
        }
        if i_pressed {
            self.show_info_badge = !self.show_info_badge;
        }
    }

    pub fn handle_drag_and_drop(&mut self, ctx: &Context) {
        if self.settings_dialog.is_open {
            return;
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            if let Some(path) = file.path {
                self.open_target(&path, ctx);
                break;
            }
        }
    }

    fn record_settings_close(&mut self, ctx: &Context) {
        self.settings_held_keys = ctx.input(|input| input.keys_down.clone());
        self.ignore_canvas_double_click_until = ctx.input(|input| input.time)
            + ctx.options(|options| options.input_options.max_double_click_delay);
    }

    fn canvas_double_click_ready(&self, ctx: &Context) -> bool {
        ctx.input(|input| input.time) >= self.ignore_canvas_double_click_until
    }

    fn apply_settings_changes(&mut self, ctx: &Context, changes: SettingsChanges) {
        if changes.filter {
            if let Err(error) = self
                .canvas_state
                .set_filter_mode(ctx, self.config.rendering.filter_mode)
            {
                self.image_upload_failed(error);
            }
        }
        if changes.cache {
            self.cache.set_capacity(
                self.config.cache.max_ram_cache_mb,
                self.config.cache.thumbnail_cache_count,
            );
        }
        if changes.window_mode && self.window_mode != self.config.window.mode {
            self.toggle_window_mode(ctx);
        }
    }

    pub fn update_ui_fade(&mut self, ctx: &Context, cursor_pos: Option<Pos2>, dt: f32) {
        let mouse_moved =
            ctx.input(|i| i.pointer.delta() != Vec2::ZERO || i.raw_scroll_delta != Vec2::ZERO);
        if mouse_moved || self.canvas_state.is_dragging {
            self.last_mouse_activity = Instant::now();
        }

        let elapsed = self.last_mouse_activity.elapsed().as_secs_f32();
        let timeout = self.config.window.ui_fade_timeout_secs.max(0.0);
        if self.config.window.auto_hide_ui && elapsed < timeout && !self.settings_dialog.is_open {
            ctx.request_repaint_after(Duration::from_secs_f32(timeout - elapsed));
        }
        let target_opacity = if !self.config.window.auto_hide_ui
            || self.settings_dialog.is_open
            || elapsed < timeout
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
        self.ui_opacity +=
            (target_opacity - self.ui_opacity) * fade_speed * dt.clamp(0.0, 1.0 / 30.0);
        if (self.ui_opacity - target_opacity).abs() > 0.01 {
            ctx.request_repaint();
        } else {
            self.ui_opacity = target_opacity;
        }
    }
}

impl eframe::App for ImageViewerApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.presentation.opaque_window() {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    }

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let ru = self.config.general.language.is_russian();
        let tr = |english: &'static str, russian: &'static str| if ru { russian } else { english };
        let dt = ctx.input(|i| i.stable_dt).max(0.001);
        let cursor_pos = ctx.input(|i| i.pointer.hover_pos());

        // Process async worker responses
        self.process_queued_open(ctx);
        self.process_scan_results();
        self.process_incoming_results(ctx);
        // Handle hotkeys & drag-and-drop
        let settings_was_open = self.settings_dialog.is_open;
        self.handle_keyboard_inputs(ctx);
        let viewer_blocked = settings_was_open || self.settings_dialog.is_open;
        if !viewer_blocked
            && ctx.input(|input| input.key_pressed(egui::Key::Space))
            && self
                .current_image
                .as_ref()
                .is_some_and(|image| !image.animation_frames.is_empty())
        {
            self.animation_paused = !self.animation_paused;
            self.animation_deadline = Instant::now();
            self.show_toast(if self.animation_paused {
                self.tr("Animation paused", "Анимация на паузе")
            } else {
                self.tr("Animation playing", "Анимация воспроизводится")
            });
        }
        if !viewer_blocked {
            self.handle_drag_and_drop(ctx);
        }

        // Resolve cached opens and queued preview/full results before touching GPU resources.
        self.upload_pending_texture(ctx);
        if !self.animation_paused {
            let next_frame = self.current_image.as_ref().and_then(|image| {
                let frames = &image.animation_frames;
                if frames.is_empty() || Instant::now() < self.animation_deadline {
                    None
                } else {
                    self.animation_frame_index = (self.animation_frame_index + 1) % frames.len();
                    Some(frames[self.animation_frame_index].clone())
                }
            });
            if let Some(frame) = next_frame {
                self.animation_deadline =
                    Instant::now() + frame.delay.div_f32(self.animation_speed.clamp(0.25, 4.0));
                if let Err(error) = self
                    .canvas_state
                    .update_animation_frame(ctx, frame.color_image)
                {
                    self.image_upload_failed(error);
                }
            }
            if self
                .current_image
                .as_ref()
                .is_some_and(|image| !image.animation_frames.is_empty())
            {
                ctx.request_repaint_after(
                    self.animation_deadline
                        .saturating_duration_since(Instant::now()),
                );
            }
        }

        // Manage UI auto-fade
        self.update_ui_fade(ctx, cursor_pos, dt);

        // Draw background — overlay uses semi-transparent dark for see-through darkening,
        // windowed respects transparent_windowed_background config.
        let bg_color = if !self.presentation.opaque_window()
            && (self.window_mode == WindowMode::Overlay
                || self.config.window.transparent_windowed_background)
        {
            let c = self.config.window.bg_color;
            Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
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
                    CanvasInput {
                        enabled: !viewer_blocked,
                        wheel_zoom: !viewer_blocked && !pointer_over_filmstrip,
                    },
                    (!self.is_loading && self.load_error.is_none()).then(|| {
                        tr(
                            "Open an image or drop it here",
                            "Откройте изображение или перетащите его сюда",
                        )
                    }),
                );

                // Double-click on canvas action
                if !viewer_blocked
                    && self.canvas_double_click_ready(ctx)
                    && canvas_resp.double_clicked
                {
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

                // Disable controls without tinting the image or its crossfade.
                if viewer_blocked {
                    ui.disable();
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
                match film_resp.scroll_nav {
                    ScrollNav::Next => self.navigate_next(),
                    ScrollNav::Prev => self.navigate_prev(),
                    ScrollNav::None => {}
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
                        tr("Open image (O)", "Открыть изображение (O)"),
                        self.ui_opacity,
                    ) {
                        if let Some(file) = rfd::FileDialog::new()
                            .add_filter("Images", SUPPORTED_IMAGE_EXTENSIONS)
                            .pick_file()
                        {
                            self.open_target(&file, ctx);
                        }
                    }
                    if icon_button(
                        ui,
                        fit_rect,
                        "fit",
                        ToolbarIcon::Fit,
                        tr("Fit to window (F)", "Вписать в окно (F)"),
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
                        tr("Settings (S)", "Настройки (S)"),
                        self.ui_opacity,
                    ) {
                        self.settings_dialog.is_open = true;
                    }
                    if icon_button(
                        ui,
                        window_rect,
                        "window-mode",
                        ToolbarIcon::Window,
                        tr("Toggle window mode (F11)", "Переключить режим окна (F11)"),
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
                            tr("Close", "Закрыть"),
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
                            FilterMode::Auto => tr("Auto", "Авто"),
                            FilterMode::Nearest => tr("Nearest", "Без сглаживания"),
                            FilterMode::Bilinear => tr("Bilinear", "Билинейный"),
                        };

                        let animation_status = (!img.animation_frames.is_empty()).then(|| {
                            format!(
                                " • {}/{} {}",
                                self.animation_frame_index + 1,
                                img.animation_frames.len(),
                                if self.animation_paused { "⏸" } else { "▶" }
                            )
                        });
                        let info_text = format!(
                            "{}×{} • {}% • {}{}",
                            img.width,
                            img.height,
                            zoom_pct,
                            filter_name,
                            animation_status.as_deref().unwrap_or_default()
                        );
                        let badge_rect = Rect::from_min_size(
                            Pos2::new(
                                viewport.min.x + 20.0,
                                viewport.max.y
                                    - if img.animation_frames.is_empty() {
                                        40.0
                                    } else {
                                        88.0
                                    },
                            ),
                            Vec2::new(
                                if img.animation_frames.is_empty() {
                                    200.0
                                } else {
                                    280.0
                                },
                                26.0,
                            ),
                        );

                        ui.painter().rect(
                            badge_rect,
                            6.0_f32,
                            Color32::from_rgba_unmultiplied(18, 18, 22, badge_alpha),
                            Stroke::NONE,
                        );

                        ui.painter().text(
                            badge_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            info_text,
                            egui::FontId::proportional(12.0),
                            Color32::from_rgba_unmultiplied(220, 220, 230, badge_alpha),
                        );
                    }
                }

                // 6. Toast notification
                if let Some((ref msg, time)) = self.status_message {
                    let elapsed = time.elapsed().as_secs_f32();
                    if elapsed < 2.0 {
                        let toast_alpha = ((1.0 - (elapsed / 2.0).powi(2)) * 240.0) as u8;
                        let toast_pos = Pos2::new(viewport.center().x, viewport.min.y + 64.0);
                        let text_rect = Rect::from_center_size(toast_pos, Vec2::new(280.0, 32.0));

                        ui.painter().rect(
                            text_rect,
                            8.0_f32,
                            Color32::from_rgba_unmultiplied(24, 24, 28, toast_alpha),
                            Stroke::new(
                                1.0_f32,
                                Color32::from_rgba_unmultiplied(255, 255, 255, toast_alpha / 4),
                            ),
                        );
                        ui.painter().text(
                            toast_pos,
                            egui::Align2::CENTER_CENTER,
                            msg,
                            egui::FontId::proportional(13.0),
                            Color32::from_rgba_unmultiplied(255, 255, 255, toast_alpha),
                        );
                        ctx.request_repaint();
                    } else {
                        self.status_message = None;
                    }
                }
            });

        if let Some(frame_count) = self
            .current_image
            .as_ref()
            .map(|image| image.animation_frames.len())
            .filter(|count| *count > 1)
        {
            egui::Area::new(egui::Id::new("animation-controls"))
                .anchor(egui::Align2::LEFT_BOTTOM, Vec2::new(16.0, -16.0))
                .show(ctx, |ui| {
                    if viewer_blocked {
                        ui.disable();
                    }
                    egui::Frame::none()
                        .fill(Color32::from_rgba_unmultiplied(18, 20, 25, 232))
                        .rounding(8.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.spacing_mut().interact_size.y = 44.0;
                            ui.horizontal(|ui| {
                                if ui
                                    .button(if self.animation_paused {
                                        tr("Play", "Воспроизвести")
                                    } else {
                                        tr("Pause", "Пауза")
                                    })
                                    .clicked()
                                {
                                    self.animation_paused = !self.animation_paused;
                                    self.animation_deadline = Instant::now();
                                }
                                ui.label(format!(
                                    "{} / {}",
                                    self.animation_frame_index + 1,
                                    frame_count
                                ));
                                ui.add(
                                    egui::Slider::new(&mut self.animation_speed, 0.25..=4.0)
                                        .logarithmic(true)
                                        .suffix("×")
                                        .show_value(true),
                                );
                            });
                        });
                });
        }

        if let Some(error) = self.load_error.clone() {
            let mut retry = false;
            let mut open_another = false;
            egui::Window::new(tr("Could not open image", "Не удалось открыть изображение"))
                .id(egui::Id::new("load-error"))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .fixed_size(Vec2::new(420.0, 150.0))
                .show(ctx, |ui| {
                    if viewer_blocked {
                        ui.disable();
                    }
                    ui.spacing_mut().interact_size.y = 44.0;
                    if let Some(path) = self.current_image_path.as_ref() {
                        ui.strong(path.file_name().unwrap_or_default().to_string_lossy());
                    }
                    ui.colored_label(Color32::from_rgb(248, 113, 113), error);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        retry = ui.button(tr("Retry", "Повторить")).clicked();
                        open_another = ui.button(tr("Open another…", "Открыть другое…")).clicked();
                    });
                });
            if retry {
                if let Some(path) = self.current_image_path.clone() {
                    self.cache.clear();
                    self.open_target(&path, ctx);
                }
            }
            if open_another {
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("Images", SUPPORTED_IMAGE_EXTENSIONS)
                    .pick_file()
                {
                    self.open_target(&file, ctx);
                }
            }
        }

        // 7. Settings Dialog
        let settings_open_before_show = self.settings_dialog.is_open;
        let outcome =
            self.settings_dialog
                .show(ctx, &mut self.config, self.presentation.opaque_window());
        if settings_open_before_show && !self.settings_dialog.is_open {
            self.record_settings_close(ctx);
        }
        self.apply_settings_changes(ctx, outcome.changes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> (ImageViewerApp, Context) {
        let ctx = Context::default();
        let app = ImageViewerApp::with_context(
            &ctx,
            None,
            ViewerConfig::default(),
            PresentationPolicy::with_opaque_window(false),
        );
        (app, ctx)
    }

    fn image(frame_count: usize) -> Arc<DecodedImage> {
        let pixels = Arc::new(egui::ColorImage::new([2, 2], Color32::WHITE));
        Arc::new(DecodedImage {
            width: 2,
            height: 2,
            bytes_size: 16,
            color_image: Arc::clone(&pixels),
            animation_frames: (0..frame_count)
                .map(|_| crate::loader::decoder::AnimationFrame {
                    color_image: Arc::clone(&pixels),
                    delay: Duration::from_millis(100),
                })
                .collect::<Vec<_>>()
                .into(),
        })
    }

    #[test]
    fn cached_activation_resets_animation_and_clears_pending_load() {
        let (mut app, _) = test_app();
        app.animation_frame_index = 12;
        app.animation_paused = true;
        app.animation_deadline = Instant::now() - Duration::from_secs(1);
        app.is_loading = true;
        app.load_error = Some("previous error".into());
        app.pending_open_path = Some(PathBuf::from("cached.gif"));
        app.activate_image(PathBuf::from("cached.gif"), image(2));
        assert_eq!(app.animation_frame_index, 0);
        assert!(!app.animation_paused);
        assert!(app.animation_deadline > Instant::now());
        assert!(!app.is_loading);
        assert!(app.pending_open_path.is_none());
        assert!(app.load_error.is_none());
        assert!(app.texture_dirty);
    }

    #[test]
    fn reopening_a_cached_file_activates_without_waiting_for_worker_result() {
        let (mut app, ctx) = test_app();
        let path = std::env::temp_dir().join(format!("viewlume-reopen-{}.png", std::process::id()));
        std::fs::write(&path, b"cache fingerprint fixture").unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        let cached = image(0);
        app.cache.insert_full(path.clone(), Arc::clone(&cached));
        app.open_target(&path, &ctx);
        app.process_queued_open(&ctx);
        assert!(Arc::ptr_eq(app.current_image.as_ref().unwrap(), &cached));
        assert_eq!(app.current_image_path.as_ref(), Some(&path));
        assert!(!app.is_loading);
        assert!(app.pending_open_path.is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn preview_to_full_keeps_the_existing_view() {
        let (mut app, _) = test_app();
        let path = PathBuf::from("image.png");
        app.current_image_is_preview = true;
        app.current_image_path = Some(path.clone());
        app.activate_image(path, image(0));
        assert!(!app.texture_reset_view);
        assert!(!app.texture_animate);
    }

    #[test]
    fn old_worker_results_cannot_replace_a_new_cached_image() {
        let (mut app, ctx) = test_app();
        let path = PathBuf::from("selected.gif");
        let selected = image(2);
        app.navigator.files = vec![path.clone()];
        app.activate_image(path.clone(), Arc::clone(&selected));
        let old_epoch = app.pipeline.current_epoch();
        app.pipeline.next_epoch();
        app.pending_load_results.push_back(LoadResult::FullImage {
            path,
            index: 0,
            epoch: old_epoch,
            result: Ok(image(10)),
        });
        app.process_incoming_results(&ctx);
        assert!(Arc::ptr_eq(app.current_image.as_ref().unwrap(), &selected));
        assert_eq!(app.animation_frame_index, 0);
    }

    #[test]
    fn cached_navigation_resets_shorter_animation() {
        let (mut app, _) = test_app();
        let path = std::env::temp_dir().join(format!(
            "viewlume-animation-cache-{}.gif",
            std::process::id()
        ));
        std::fs::write(&path, b"fingerprint").unwrap();
        app.navigator.files = vec![path.clone()];
        let cached = image(2);
        app.cache.insert_full(path.clone(), Arc::clone(&cached));
        app.current_image = Some(image(12));
        app.animation_frame_index = 11;
        app.animation_paused = true;
        app.load_current_image(true);
        assert!(Arc::ptr_eq(app.current_image.as_ref().unwrap(), &cached));
        assert_eq!(app.animation_frame_index, 0);
        assert!(!app.animation_paused);
        assert!(!app.is_loading);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn settings_block_viewer_keys_and_file_drops() {
        let (mut app, ctx) = test_app();
        app.settings_dialog.is_open = true;
        app.navigator.files = vec![PathBuf::from("one.png"), PathBuf::from("two.png")];
        let original_mode = app.window_mode;
        let original_filter = app.config.rendering.filter_mode;
        let events = [
            egui::Key::ArrowRight,
            egui::Key::F11,
            egui::Key::N,
            egui::Key::Escape,
            egui::Key::S,
        ]
        .into_iter()
        .map(|key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        })
        .collect();
        let input = egui::RawInput {
            events,
            dropped_files: vec![egui::DroppedFile {
                path: Some(PathBuf::from("replacement.png")),
                ..Default::default()
            }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            app.handle_keyboard_inputs(ctx);
            app.handle_drag_and_drop(ctx);
        });
        assert_eq!(app.navigator.current_index, 0);
        assert_eq!(app.window_mode, original_mode);
        assert_eq!(app.config.rendering.filter_mode, original_filter);
        assert_eq!(app.scan_generation, 0);
        assert!(app.settings_dialog.is_open);
    }

    #[test]
    fn inactivity_schedules_a_future_repaint_and_fade_settles() {
        let (mut app, ctx) = test_app();
        app.config.window.ui_fade_timeout_secs = 2.0;
        for _ in 0..3 {
            let _ = ctx.run(Default::default(), |ctx| {
                app.update_ui_fade(ctx, Some(Pos2::new(640.0, 400.0)), 0.016);
            });
        }
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
                ..Default::default()
            },
            |ctx| {
                app.update_ui_fade(ctx, Some(Pos2::new(640.0, 400.0)), 0.016);
            },
        );
        let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        assert!(
            delay > Duration::ZERO && delay <= Duration::from_secs(2),
            "{delay:?}"
        );
        app.last_mouse_activity = Instant::now() - Duration::from_secs(3);
        let _ = ctx.run(Default::default(), |ctx| {
            app.update_ui_fade(ctx, Some(Pos2::new(640.0, 400.0)), 1.0);
        });
        assert!(app.ui_opacity > 0.0 && app.ui_opacity < 1.0);
        for _ in 0..90 {
            let _ = ctx.run(Default::default(), |ctx| {
                app.update_ui_fade(ctx, Some(Pos2::new(640.0, 400.0)), 1.0 / 60.0);
            });
        }
        assert_eq!(app.ui_opacity, 0.0);
    }

    #[test]
    fn opaque_native_policy_always_clears_opaque() {
        let (mut app, _) = test_app();
        use eframe::App;
        assert_eq!(app.clear_color(&egui::Visuals::dark())[3], 0.0);
        app.presentation = PresentationPolicy::with_opaque_window(true);
        assert_eq!(app.clear_color(&egui::Visuals::dark())[3], 1.0);
    }

    #[test]
    fn runtime_window_toggles_do_not_change_the_saved_default() {
        let (mut app, ctx) = test_app();
        let default_mode = app.config.window.mode;
        app.toggle_window_mode(&ctx);
        let runtime_mode = app.window_mode;
        assert_ne!(runtime_mode, default_mode);
        assert_eq!(app.config.window.mode, default_mode);
        app.apply_settings_changes(
            &ctx,
            SettingsChanges {
                any: true,
                ..Default::default()
            },
        );
        assert_eq!(app.window_mode, runtime_mode);
        app.apply_settings_changes(
            &ctx,
            SettingsChanges {
                window_mode: true,
                ..Default::default()
            },
        );
        assert_eq!(app.window_mode, default_mode);
    }

    #[test]
    fn unrelated_settings_do_not_upload_image_pixels() {
        let (mut app, ctx) = test_app();
        app.activate_image(PathBuf::from("image.png"), image(0));
        let _ = ctx.run(Default::default(), |ctx| app.upload_pending_texture(ctx));
        let image_texture = app.canvas_state.texture.as_ref().unwrap().id();
        let output = ctx.run(Default::default(), |ctx| {
            app.apply_settings_changes(
                ctx,
                SettingsChanges {
                    any: true,
                    ..Default::default()
                },
            );
        });
        assert!(output
            .textures_delta
            .set
            .iter()
            .all(|(id, _)| *id != image_texture));
    }

    #[test]
    fn a_filter_setting_updates_the_displayed_texture_once() {
        let (mut app, ctx) = test_app();
        app.activate_image(PathBuf::from("image.png"), image(0));
        let _ = ctx.run(Default::default(), |ctx| app.upload_pending_texture(ctx));
        let image_texture = app.canvas_state.texture.as_ref().unwrap().id();
        app.config.rendering.filter_mode = crate::config::FilterMode::Nearest;
        let changes = SettingsChanges {
            any: true,
            filter: true,
            ..Default::default()
        };
        let output = ctx.run(Default::default(), |ctx| {
            app.apply_settings_changes(ctx, changes)
        });
        assert_eq!(
            output
                .textures_delta
                .set
                .iter()
                .filter(|(id, _)| *id == image_texture)
                .count(),
            1
        );
        let output = ctx.run(Default::default(), |ctx| {
            app.apply_settings_changes(ctx, changes)
        });
        assert!(output
            .textures_delta
            .set
            .iter()
            .all(|(id, _)| *id != image_texture));
    }

    #[test]
    fn settings_close_disarms_the_second_click_of_a_double_click() {
        let (mut app, ctx) = test_app();
        let _ = ctx.run(
            egui::RawInput {
                time: Some(10.0),
                ..Default::default()
            },
            |ctx| {
                app.record_settings_close(ctx);
                assert!(!app.canvas_double_click_ready(ctx));
            },
        );
        let _ = ctx.run(
            egui::RawInput {
                time: Some(10.1),
                ..Default::default()
            },
            |ctx| {
                assert!(!app.canvas_double_click_ready(ctx));
            },
        );
        let _ = ctx.run(
            egui::RawInput {
                time: Some(10.5),
                ..Default::default()
            },
            |ctx| {
                assert!(app.canvas_double_click_ready(ctx));
            },
        );
    }

    #[test]
    fn held_escape_cannot_toggle_the_viewer_after_settings_closes() {
        let (mut app, ctx) = test_app();
        let initial_mode = app.window_mode;
        app.settings_dialog.is_open = true;
        let initial_press = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            ..Default::default()
        };
        let _ = ctx.run(initial_press, |ctx| app.handle_keyboard_inputs(ctx));
        app.settings_dialog.is_open = false;
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: true,
                modifiers: Default::default(),
            }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.handle_keyboard_inputs(ctx));
        assert_eq!(app.window_mode, initial_mode);
    }

    #[test]
    fn keys_held_in_settings_need_a_fresh_press_before_navigation() {
        let (mut app, ctx) = test_app();
        app.navigator.files = vec![PathBuf::from("one.png"), PathBuf::from("two.png")];
        app.settings_dialog.is_open = true;
        let key_input = |pressed, repeat| egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed,
                repeat,
                modifiers: Default::default(),
            }],
            ..Default::default()
        };
        let _ = ctx.run(key_input(true, false), |ctx| {
            app.handle_keyboard_inputs(ctx)
        });
        app.settings_dialog.is_open = false;
        app.record_settings_close(&ctx);
        let _ = ctx.run(key_input(true, true), |ctx| app.handle_keyboard_inputs(ctx));
        assert_eq!(app.navigator.current_index, 0);
        let _ = ctx.run(key_input(false, false), |ctx| {
            app.handle_keyboard_inputs(ctx)
        });
        let _ = ctx.run(key_input(true, false), |ctx| {
            app.handle_keyboard_inputs(ctx)
        });
        assert_eq!(app.navigator.current_index, 1);
    }

    #[test]
    fn invalid_gpu_upload_becomes_an_image_error() {
        let (mut app, ctx) = test_app();
        let mut oversized = (*image(0)).clone();
        let side = ctx.input(|input| input.max_texture_side) + 1;
        oversized.color_image = Arc::new(egui::ColorImage::new([side, 1], Color32::WHITE));
        app.activate_image(PathBuf::from("panorama.png"), Arc::new(oversized));
        app.upload_pending_texture(&ctx);
        assert!(app.load_error.is_some());
        assert!(app.current_image.is_none());
        assert!(app.canvas_state.texture.is_none());
        assert!(!app.is_loading);
    }

    #[test]
    fn queued_preview_and_full_image_upload_only_the_final_pixels() {
        let (mut app, ctx) = test_app();
        let path = PathBuf::from("image.png");
        app.navigator.files = vec![path.clone()];
        let epoch = app.pipeline.current_epoch();
        let full = image(0);
        app.pending_load_results.push_back(LoadResult::Preview {
            path: path.clone(),
            index: 0,
            epoch,
            width: 2,
            height: 2,
            image: Arc::new(egui::ColorImage::new([1, 1], Color32::GRAY)),
        });
        app.pending_load_results.push_back(LoadResult::FullImage {
            path,
            index: 0,
            epoch,
            result: Ok(Arc::clone(&full)),
        });
        let output = ctx.run(Default::default(), |ctx| {
            app.process_incoming_results(ctx);
            assert!(
                app.canvas_state.texture.is_none(),
                "result polling must not allocate textures"
            );
            app.upload_pending_texture(ctx);
        });
        let id = app.canvas_state.texture.as_ref().unwrap().id();
        assert_eq!(app.canvas_state.texture.as_ref().unwrap().size(), [2, 2]);
        assert_eq!(
            output
                .textures_delta
                .set
                .iter()
                .filter(|(texture, _)| *texture == id)
                .count(),
            1
        );
        assert!(output.textures_delta.free.is_empty());
        assert!(Arc::ptr_eq(app.current_image.as_ref().unwrap(), &full));
    }

    #[test]
    fn upload_error_does_not_free_a_texture_with_pending_writes() {
        let (mut app, ctx) = test_app();
        let output = ctx.run(Default::default(), |ctx| {
            app.activate_image(PathBuf::from("valid.png"), image(0));
            app.upload_pending_texture(ctx);
            let mut oversized = (*image(0)).clone();
            let side = ctx.input(|input| input.max_texture_side) + 1;
            oversized.color_image = Arc::new(egui::ColorImage::new([side, 1], Color32::WHITE));
            app.activate_image(PathBuf::from("oversized.png"), Arc::new(oversized));
            app.upload_pending_texture(ctx);
        });
        assert!(app.load_error.is_some());
        assert!(app.current_image.is_none());
        assert!(
            app.canvas_state.texture.is_some(),
            "retain bounded GPU assets until safe reuse"
        );
        for id in output.textures_delta.free {
            assert!(
                output.textures_delta.set.iter().all(|(set, _)| *set != id),
                "freeing an uploaded managed texture before submit destroys its queued writes"
            );
        }
    }

    #[test]
    fn open_actions_retain_uploaded_and_painted_thumbnails_until_next_frame() {
        let (mut app, ctx) = test_app();
        let path = PathBuf::from("thumbnail.png");
        let mut texture_id = egui::TextureId::default();
        let output = ctx.run(Default::default(), |ctx| {
            app.filmstrip_state.insert_loaded_thumbnail(
                ctx,
                path.clone(),
                0,
                Arc::new(egui::ColorImage::new([2, 2], Color32::WHITE)),
            );
            texture_id = app.filmstrip_state.thumb_textures[&path].id();
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.painter().image(
                    texture_id,
                    Rect::from_min_size(Pos2::ZERO, Vec2::splat(20.0)),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
                app.open_target(Path::new("next-folder"), ctx);
            });
            assert!(app.filmstrip_state.thumb_textures.contains_key(&path));
        });
        assert!(output
            .textures_delta
            .set
            .iter()
            .any(|(id, _)| *id == texture_id));
        assert!(!output.textures_delta.free.contains(&texture_id));
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        assert!(primitives.iter().any(|primitive| matches!(
            &primitive.primitive, egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == texture_id
        )));
        let output = ctx.run(Default::default(), |ctx| app.process_queued_open(ctx));
        assert!(!app.filmstrip_state.thumb_textures.contains_key(&path));
        assert!(output.textures_delta.free.contains(&texture_id));
        assert!(output
            .textures_delta
            .set
            .iter()
            .all(|(id, _)| *id != texture_id));
    }

    #[test]
    fn rapid_navigation_expands_prefetch_in_travel_direction() {
        let start = Instant::now();
        let mut pace = NavigationPace::default();
        pace.record_at(1, start);
        assert_eq!(pace.prefetch_counts(2), (2, 2));

        pace.record_at(1, start + Duration::from_millis(120));
        assert_eq!(pace.prefetch_counts(2), (4, 1));

        pace.record_at(1, start + Duration::from_millis(220));
        assert_eq!(pace.prefetch_counts(2), (6, 1));

        pace.record_at(-1, start + Duration::from_millis(300));
        assert_eq!(pace.prefetch_counts(2), (2, 2));
    }
}
