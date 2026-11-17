use egui::{
    Color32, ColorImage, Pos2, Rect, Sense, TextureHandle, TextureId, TextureOptions, Ui, Vec2,
};
use std::sync::Arc;

use crate::config::{FilterMode, RenderingConfig};
use crate::loader::decoder::DecodedImage;

struct OutgoingImage {
    texture: TextureHandle,
    rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureSizeError {
    TooLarge {
        width: usize,
        height: usize,
        max_texture_side: usize,
    },
}

impl std::fmt::Display for TextureSizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge {
                width,
                height,
                max_texture_side,
            } => write!(
                f,
                "Image texture is {width}×{height}, but this renderer supports at most {max_texture_side}×{max_texture_side}"
            ),
        }
    }
}

impl std::error::Error for TextureSizeError {}

pub struct CanvasState {
    pub pan: Vec2,
    pub scale: f64,
    pub initialized: bool,
    pub texture: Option<TextureHandle>,
    pub current_filter: FilterMode,
    pub is_dragging: bool,
    target_pan: Vec2,
    target_scale: f64,
    pan_velocity: Vec2,
    preserved_extent: Option<f32>,
    preserved_center: Option<Vec2>,
    current_image_size: Vec2,
    outgoing: Option<OutgoingImage>,
    transition_progress: f32,
    transition_direction: f32,
    texture_sequence: u64,
    texture_options: Option<TextureOptions>,
    displayed_pixels: Option<Arc<ColorImage>>,
    checkerboard: Option<TextureHandle>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            scale: 1.0,
            initialized: false,
            texture: None,
            current_filter: FilterMode::Auto,
            is_dragging: false,
            target_pan: Vec2::ZERO,
            target_scale: 1.0,
            pan_velocity: Vec2::ZERO,
            preserved_extent: None,
            preserved_center: None,
            current_image_size: Vec2::ZERO,
            outgoing: None,
            transition_progress: 1.0,
            transition_direction: 1.0,
            texture_sequence: 0,
            texture_options: None,
            displayed_pixels: None,
            checkerboard: None,
        }
    }
}

impl CanvasState {
    fn fit_scale(img_w: f32, img_h: f32, viewport: Rect) -> f64 {
        let margin = 24.0;
        let avail_w = (viewport.width() - margin * 2.0).max(10.0);
        let avail_h = (viewport.height() - margin * 2.0).max(10.0);
        (avail_w / img_w).min(avail_h / img_h).max(0.002) as f64
    }

    fn set_view(
        &mut self,
        img_w: f32,
        img_h: f32,
        viewport: Rect,
        scale: f64,
        center: Option<Vec2>,
    ) {
        self.scale = scale;
        self.target_scale = scale;
        self.current_image_size = Vec2::new(img_w, img_h);
        let size = self.current_image_size * scale as f32;
        let center = center.unwrap_or_else(|| viewport.center().to_vec2());
        self.pan = center - size * 0.5;
        self.target_pan = self.pan;
        self.pan_velocity = Vec2::ZERO;
        self.initialized = true;
    }

    pub fn reset_view_fit(&mut self, img_w: f32, img_h: f32, viewport: Rect) {
        if img_w <= 0.0 || img_h <= 0.0 || viewport.width() <= 0.0 || viewport.height() <= 0.0 {
            return;
        }
        self.preserved_extent = None;
        self.preserved_center = None;
        self.set_view(
            img_w,
            img_h,
            viewport,
            Self::fit_scale(img_w, img_h, viewport),
            None,
        );
    }

    pub fn reset_view_actual_size(&mut self, img_w: f32, img_h: f32, viewport: Rect) {
        self.preserved_extent = None;
        self.preserved_center = None;
        self.set_view(img_w, img_h, viewport, 1.0, None);
    }

    /// Preserve both visual size and the on-screen center across image changes.
    pub fn prepare_image_change(&mut self, image: Option<&DecodedImage>) {
        if let Some(image) = image {
            let rendered_size =
                Vec2::new(image.width as f32, image.height as f32) * self.target_scale as f32;
            self.preserved_extent = Some(rendered_size.x.max(rendered_size.y));
            self.preserved_center = Some(self.target_pan + rendered_size * 0.5);
        }
        self.pan_velocity = Vec2::ZERO;
    }

    pub fn set_transition_direction(&mut self, direction: f32) {
        self.transition_direction = direction.signum();
    }

    fn initialize_image(&mut self, img_w: f32, img_h: f32, viewport: Rect, cfg: &RenderingConfig) {
        let scale = self
            .preserved_extent
            .take()
            .map(|extent| f64::from(extent / img_w.max(img_h)))
            .unwrap_or_else(|| Self::fit_scale(img_w, img_h, viewport))
            .clamp(cfg.min_zoom, cfg.max_zoom);
        let center = self.preserved_center.take();
        self.set_view(img_w, img_h, viewport, scale, center);
    }

    pub fn get_texture_options(filter_mode: FilterMode, _scale: f64) -> TextureOptions {
        match filter_mode {
            FilterMode::Nearest => TextureOptions::NEAREST,
            FilterMode::Bilinear => TextureOptions::LINEAR,
            FilterMode::Auto => TextureOptions {
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Linear,
                wrap_mode: egui::TextureWrapMode::ClampToEdge,
                mipmap_mode: None,
            },
        }
    }

    fn texture_options_changed(&self, options: TextureOptions) -> bool {
        self.texture_options != Some(options)
    }

    fn validate_texture_size(
        ctx: &egui::Context,
        pixels: &ColorImage,
    ) -> Result<(), TextureSizeError> {
        let [width, height] = pixels.size;
        let max_texture_side = ctx.input(|input| input.max_texture_side);
        if width > max_texture_side || height > max_texture_side {
            return Err(TextureSizeError::TooLarge {
                width,
                height,
                max_texture_side,
            });
        }
        Ok(())
    }

    fn upload_pixels(
        &mut self,
        ctx: &egui::Context,
        pixels: Arc<ColorImage>,
        options: TextureOptions,
        needs_new_texture: bool,
    ) -> Result<(), TextureSizeError> {
        Self::validate_texture_size(ctx, &pixels)?;

        let size_changed = self
            .texture
            .as_ref()
            .is_none_or(|texture| texture.size() != pixels.size);
        if needs_new_texture || size_changed {
            self.texture_sequence += 1;
            self.texture = Some(ctx.load_texture(
                format!("active-image-{}", self.texture_sequence),
                pixels.clone(),
                options,
            ));
        } else if let Some(texture) = self.texture.as_mut() {
            texture.set_partial([0, 0], pixels.clone(), options);
        }
        self.texture_options = Some(options);
        self.displayed_pixels = Some(pixels);
        Ok(())
    }

    pub fn update_texture(
        &mut self,
        ctx: &egui::Context,
        image: &Arc<DecodedImage>,
        filter_mode: FilterMode,
        is_new_image: bool,
        animate: bool,
    ) -> Result<(), TextureSizeError> {
        let options = Self::get_texture_options(filter_mode, self.target_scale);
        Self::validate_texture_size(ctx, &image.color_image)?;

        // Filter changes apply to the pixels on screen. In particular, do not replace an
        // active animation frame with the decoded still image when a shortcut is pressed.
        if !is_new_image && self.current_filter != filter_mode {
            return self.set_filter_mode(ctx, filter_mode);
        }

        let pixels_changed = self
            .displayed_pixels
            .as_ref()
            .is_none_or(|displayed| !Arc::ptr_eq(displayed, &image.color_image));
        let options_changed = self.texture_options_changed(options);
        if is_new_image {
            if animate {
                if let Some(texture) = self.texture.take() {
                    let size = self.current_image_size * self.target_scale as f32;
                    self.outgoing = Some(OutgoingImage {
                        texture,
                        rect: Rect::from_min_size(self.target_pan.to_pos2(), size),
                    });
                    self.transition_progress = 0.0;
                }
            } else {
                self.outgoing = None;
                self.transition_progress = 1.0;
            }
            let needs_new_texture = animate && self.outgoing.is_some();
            self.upload_pixels(ctx, image.color_image.clone(), options, needs_new_texture)?;
            self.initialized = false;
        } else if pixels_changed || options_changed || self.texture.is_none() {
            self.upload_pixels(ctx, image.color_image.clone(), options, false)?;
        }
        self.current_filter = filter_mode;
        Ok(())
    }

    pub fn set_filter_mode(
        &mut self,
        ctx: &egui::Context,
        filter_mode: FilterMode,
    ) -> Result<(), TextureSizeError> {
        self.current_filter = filter_mode;
        self.sync_texture_options(ctx)
    }

    fn sync_texture_options(&mut self, ctx: &egui::Context) -> Result<(), TextureSizeError> {
        let filter_mode = self.current_filter;
        let options = Self::get_texture_options(filter_mode, self.target_scale);
        if self.texture_options_changed(options) {
            if let Some(pixels) = self.displayed_pixels.clone() {
                self.upload_pixels(ctx, pixels, options, false)?;
            }
        }
        Ok(())
    }

    pub fn update_animation_frame(
        &mut self,
        ctx: &egui::Context,
        frame: Arc<ColorImage>,
    ) -> Result<(), TextureSizeError> {
        let options = Self::get_texture_options(self.current_filter, self.target_scale);
        self.upload_pixels(ctx, frame, options, false)
    }

    fn checkerboard_texture(&mut self, ctx: &egui::Context) -> TextureId {
        let texture = self.checkerboard.get_or_insert_with(|| {
            let mut image = ColorImage::new([2, 2], Color32::from_gray(32));
            image.pixels = vec![
                Color32::from_gray(32),
                Color32::from_gray(22),
                Color32::from_gray(22),
                Color32::from_gray(32),
            ];
            ctx.load_texture(
                "checkerboard",
                image,
                TextureOptions {
                    magnification: egui::TextureFilter::Nearest,
                    minification: egui::TextureFilter::Nearest,
                    wrap_mode: egui::TextureWrapMode::Repeat,
                    mipmap_mode: None,
                },
            )
        });
        texture.id()
    }

    fn animate(&mut self, dt: f32, cfg: &RenderingConfig) -> bool {
        let mut moving = false;
        let zoom_blend = 1.0 - (-24.0 * dt).exp();
        self.scale += (self.target_scale - self.scale) * zoom_blend as f64;
        if (self.target_scale - self.scale).abs() > 0.000_05 {
            moving = true;
        } else {
            self.scale = self.target_scale;
        }

        if cfg.smooth_pan && !self.is_dragging && self.pan_velocity.length_sq() > 1.0 {
            self.pan += self.pan_velocity * dt;
            self.target_pan = self.pan;
            self.pan_velocity *= (-12.5 * dt).exp();
            moving = true;
        } else if !cfg.smooth_pan {
            self.pan_velocity = Vec2::ZERO;
        }

        let pan_blend = 1.0 - (-26.0 * dt).exp();
        self.pan += (self.target_pan - self.pan) * pan_blend;
        if (self.pan - self.target_pan).length() > 0.05 {
            moving = true;
        } else if self.pan_velocity.length_sq() <= 1.0 {
            self.pan = self.target_pan;
        }

        if self.outgoing.is_some() {
            if cfg.animate_image_transitions {
                let duration = (cfg.image_transition_ms / 1000.0).clamp(0.08, 0.6);
                self.transition_progress = (self.transition_progress + dt / duration).min(1.0);
                moving |= self.transition_progress < 1.0;
            } else {
                self.transition_progress = 1.0;
            }
            if self.transition_progress >= 1.0 {
                self.outgoing = None;
            }
        }
        moving
    }
}

pub struct CanvasResponse {
    pub double_clicked: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct CanvasInput {
    pub enabled: bool,
    pub wheel_zoom: bool,
}

pub fn render_canvas(
    ui: &mut Ui,
    state: &mut CanvasState,
    current_image: Option<&Arc<DecodedImage>>,
    render_cfg: &RenderingConfig,
    show_checkerboard: bool,
    input: CanvasInput,
    empty_hint: Option<&str>,
) -> CanvasResponse {
    let allow_input = input.enabled;
    let allow_wheel_zoom = input.wheel_zoom;
    let sense = if allow_input {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), sense);
    let painter = ui.painter_at(rect);
    let cursor_pos = ui.input(|i| i.pointer.hover_pos());
    let dt = ui.input(|i| i.unstable_dt).clamp(1.0 / 240.0, 1.0 / 20.0);

    if let Some(image) = current_image {
        if !state.initialized {
            state.initialize_image(image.width as f32, image.height as f32, rect, render_cfg);
        }
        // A filter selection is applied to the current GPU pixels once. Auto options are
        // scale-independent, so all subsequent zoom work remains sampler/upload free.
        let _ = state.sync_texture_options(ui.ctx());
    }

    let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
    if scroll_delta != 0.0 && response.hovered() && allow_input && allow_wheel_zoom {
        if let Some(mouse_pos) = cursor_pos {
            let steps = (scroll_delta / 70.0).clamp(-4.0, 4.0) as f64;
            let exponent = if render_cfg.reverse_zoom_direction {
                -steps
            } else {
                steps
            };
            let old_target = state.target_scale;
            let new_target = (old_target * render_cfg.zoom_factor.powf(exponent))
                .clamp(render_cfg.min_zoom, render_cfg.max_zoom);
            if render_cfg.zoom_towards_cursor && old_target > 0.0 {
                let mouse = mouse_pos.to_vec2();
                state.target_pan =
                    mouse - (mouse - state.target_pan) * (new_target / old_target) as f32;
            }
            state.target_scale = new_target;
        }
    }

    let dragging = allow_input
        && (response.dragged_by(egui::PointerButton::Primary)
            || response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle));
    if dragging {
        // pointer.delta is per-frame; Response::drag_delta is cumulative and caused runaway panning.
        let delta = ui.input(|i| i.pointer.delta());
        state.pan += delta;
        state.target_pan = state.pan;
        state.pan_velocity = delta / dt * 0.68;
    } else if state.is_dragging {
        state.target_pan = state.pan;
    }
    state.is_dragging = dragging;

    if state.animate(dt, render_cfg) {
        ui.ctx().request_repaint();
    }

    if let Some(outgoing) = state.outgoing.as_ref() {
        let t = state.transition_progress;
        let eased = 1.0 - (1.0 - t).powi(3);
        let offset = Vec2::new(-state.transition_direction * 28.0 * eased, 0.0);
        painter.image(
            outgoing.texture.id(),
            outgoing.rect.translate(offset),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::from_white_alpha(((1.0 - eased) * 255.0) as u8),
        );
    }

    if let Some(image) = current_image {
        if let Some(texture_id) = state.texture.as_ref().map(TextureHandle::id) {
            let size = Vec2::new(image.width as f32, image.height as f32) * state.scale as f32;
            let mut image_rect = Rect::from_min_size(state.pan.to_pos2(), size);
            let transition = if state.outgoing.is_some() {
                let t = state.transition_progress;
                let eased = 1.0 - (1.0 - t).powi(3);
                image_rect = image_rect.translate(Vec2::new(
                    state.transition_direction * 28.0 * (1.0 - eased),
                    0.0,
                ));
                eased
            } else {
                1.0
            };

            if show_checkerboard && rect.intersects(image_rect) {
                let intersect = rect.intersect(image_rect);
                let cell_size = 16.0 * (state.scale as f32).clamp(0.5, 2.0);
                let tile_size = cell_size * 2.0;
                painter.image(
                    state.checkerboard_texture(ui.ctx()),
                    intersect,
                    Rect::from_min_max(
                        Pos2::ZERO,
                        Pos2::new(
                            intersect.width() / tile_size,
                            intersect.height() / tile_size,
                        ),
                    ),
                    Color32::WHITE,
                );
            }

            painter.image(
                texture_id,
                image_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::from_white_alpha((transition * 255.0) as u8),
            );
        }
    } else if let Some(empty_hint) = empty_hint {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            empty_hint,
            egui::FontId::proportional(16.0),
            Color32::from_gray(142),
        );
    }

    CanvasResponse {
        double_clicked: allow_input && response.double_clicked(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(width: u32, height: u32) -> DecodedImage {
        decoded_with_pixels(width, height, width, height, Color32::WHITE)
    }

    fn decoded_with_pixels(
        width: u32,
        height: u32,
        pixel_width: u32,
        pixel_height: u32,
        color: Color32,
    ) -> DecodedImage {
        DecodedImage {
            width,
            height,
            bytes_size: (pixel_width * pixel_height * 4) as usize,
            color_image: Arc::new(egui::ColorImage::new(
                [pixel_width as usize, pixel_height as usize],
                color,
            )),
            animation_frames: Arc::from([]),
        }
    }

    fn input(max_texture_side: usize) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            max_texture_side: Some(max_texture_side),
            ..Default::default()
        }
    }

    fn input_with_events(max_texture_side: usize, events: Vec<egui::Event>) -> egui::RawInput {
        let mut input = input(max_texture_side);
        input.events = events;
        input
    }

    fn texture_delta(output: &egui::FullOutput, id: TextureId) -> &egui::epaint::ImageDelta {
        &output
            .textures_delta
            .set
            .iter()
            .find(|(texture_id, _)| *texture_id == id)
            .expect("active texture delta")
            .1
    }

    #[test]
    fn image_change_preserves_extent_and_center_for_different_resolutions() {
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0));
        let old = decoded(1920, 1080);
        let mut state = CanvasState::default();
        state.reset_view_fit(old.width as f32, old.height as f32, viewport);
        state.target_pan += Vec2::new(73.0, -41.0);
        let old_extent = old.width as f32 * state.target_scale as f32;
        let old_center = state.target_pan
            + Vec2::new(old.width as f32, old.height as f32) * state.target_scale as f32 * 0.5;

        state.prepare_image_change(Some(&old));
        assert!(
            state.initialized,
            "old image must remain stable while loading"
        );
        state.initialized = false; // activation of the newly loaded texture
        state.initialize_image(128.0, 128.0, viewport, &RenderingConfig::default());

        let new_extent = 128.0 * state.target_scale as f32;
        let new_center = state.target_pan + Vec2::splat(new_extent * 0.5);
        assert!((old_extent - new_extent).abs() < 0.01);
        assert!((old_center - new_center).length() < 0.01);
    }

    #[test]
    fn texture_options_auto_mode() {
        let opts_scaled_up = CanvasState::get_texture_options(FilterMode::Auto, 2.0);
        assert_eq!(opts_scaled_up.magnification, egui::TextureFilter::Nearest);
        assert_eq!(opts_scaled_up.minification, egui::TextureFilter::Linear);
        assert_eq!(opts_scaled_up.mipmap_mode, None);

        let opts_scaled_down = CanvasState::get_texture_options(FilterMode::Auto, 0.5);
        assert_eq!(opts_scaled_down, opts_scaled_up);
    }

    #[test]
    fn rejects_actual_pixel_buffers_above_the_renderer_limit() {
        const LIMIT: usize = 2048;
        let ctx = egui::Context::default();
        let mut state = CanvasState::default();
        let below = Arc::new(decoded((LIMIT - 1) as u32, 1));
        let at_limit = Arc::new(decoded(LIMIT as u32, 1));
        let above = Arc::new(decoded((LIMIT + 1) as u32, 1));
        let long_narrow = Arc::new(decoded(1, (LIMIT + 1) as u32));
        // Preview metadata can retain the original dimensions while its actual upload buffer is
        // within the renderer limit.
        let small_preview = Arc::new(decoded_with_pixels(10_000, 500, 64, 4, Color32::WHITE));

        for image in [below, at_limit, small_preview] {
            let _ = ctx.run(input(LIMIT), |ctx| {
                state
                    .update_texture(ctx, &image, FilterMode::Auto, true, false)
                    .expect("buffer must fit the renderer limit");
            });
        }
        for image in [above, long_narrow] {
            let _ = ctx.run(input(LIMIT), |ctx| {
                assert_eq!(
                    state.update_texture(ctx, &image, FilterMode::Auto, true, false),
                    Err(TextureSizeError::TooLarge {
                        width: image.color_image.size[0],
                        height: image.color_image.size[1],
                        max_texture_side: LIMIT,
                    })
                );
            });
        }
    }

    #[test]
    fn unchanged_zoom_and_filter_do_not_upload_image_pixels() {
        let ctx = egui::Context::default();
        let image = Arc::new(decoded(16, 16));
        let mut state = CanvasState::default();
        let _ = ctx.run(input(2048), |ctx| {
            state
                .update_texture(ctx, &image, FilterMode::Auto, true, false)
                .expect("initial upload");
        });
        let texture_id = state.texture.as_ref().expect("active texture").id();

        state.reset_view_actual_size(
            16.0,
            16.0,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)),
        );
        state.target_scale = 2.0;
        let output = ctx.run(input(2048), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_canvas(
                    ui,
                    &mut state,
                    Some(&image),
                    &RenderingConfig::default(),
                    false,
                    CanvasInput {
                        enabled: true,
                        wheel_zoom: true,
                    },
                    None,
                );
            });
        });
        assert!(
            output
                .textures_delta
                .set
                .iter()
                .all(|(id, _)| *id != texture_id),
            "zooming with fixed Auto options must not upload image pixels"
        );

        state
            .set_filter_mode(&ctx, FilterMode::Auto)
            .expect("unchanged filter");
        let output = ctx.run(input(2048), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_canvas(
                    ui,
                    &mut state,
                    Some(&image),
                    &RenderingConfig::default(),
                    false,
                    CanvasInput {
                        enabled: true,
                        wheel_zoom: true,
                    },
                    None,
                );
            });
        });
        assert!(output
            .textures_delta
            .set
            .iter()
            .all(|(id, _)| *id != texture_id));
    }

    #[test]
    fn filter_change_keeps_the_displayed_animation_frame_and_uses_partial_upload() {
        let ctx = egui::Context::default();
        let image = Arc::new(decoded(2, 2));
        let frame = Arc::new(ColorImage::new([2, 2], Color32::BLUE));
        let mut state = CanvasState::default();
        let _ = ctx.run(input(2048), |ctx| {
            state
                .update_texture(ctx, &image, FilterMode::Auto, true, false)
                .expect("initial upload");
        });
        let texture_id = state.texture.as_ref().expect("active texture").id();

        let frame_output = ctx.run(input(2048), |ctx| {
            state
                .update_animation_frame(ctx, Arc::clone(&frame))
                .expect("same-size animation frame");
        });
        assert_eq!(texture_delta(&frame_output, texture_id).pos, Some([0, 0]));

        state
            .set_filter_mode(&ctx, FilterMode::Nearest)
            .expect("filter change");
        let filter_output = ctx.run(input(2048), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_canvas(
                    ui,
                    &mut state,
                    Some(&image),
                    &RenderingConfig::default(),
                    false,
                    CanvasInput {
                        enabled: true,
                        wheel_zoom: true,
                    },
                    None,
                );
            });
        });
        let delta = texture_delta(&filter_output, texture_id);
        assert_eq!(delta.pos, Some([0, 0]));
        match &delta.image {
            egui::ImageData::Color(pixels) => assert_eq!(pixels.pixels[0], Color32::BLUE),
            egui::ImageData::Font(_) => panic!("active image must remain a color texture"),
        }
    }

    #[test]
    fn checkerboard_shape_count_is_constant_for_large_images() {
        fn shape_count(image: Arc<DecodedImage>) -> usize {
            let ctx = egui::Context::default();
            let mut state = CanvasState::default();
            let _ = ctx.run(input(16_384), |ctx| {
                state
                    .update_texture(ctx, &image, FilterMode::Auto, true, false)
                    .expect("initial upload");
            });
            ctx.run(input(16_384), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_canvas(
                        ui,
                        &mut state,
                        Some(&image),
                        &RenderingConfig::default(),
                        true,
                        CanvasInput {
                            enabled: true,
                            wheel_zoom: true,
                        },
                        None,
                    );
                });
            })
            .shapes
            .len()
        }

        assert_eq!(
            shape_count(Arc::new(decoded(100, 100))),
            shape_count(Arc::new(decoded_with_pixels(
                10_000,
                10_000,
                2,
                2,
                Color32::WHITE,
            )))
        );
    }

    #[test]
    fn bottom_band_dragging_stays_enabled_when_only_wheel_zoom_is_blocked() {
        let ctx = egui::Context::default();
        let image = Arc::new(decoded(16, 16));
        let mut state = CanvasState::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let start = Pos2::new(400.0, 550.0);
        let mid = Pos2::new(440.0, 550.0);
        let end = Pos2::new(480.0, 550.0);

        let _ = ctx.run(input(2048), |ctx| {
            state
                .update_texture(ctx, &image, FilterMode::Auto, true, false)
                .expect("initial upload");
        });
        state.reset_view_actual_size(16.0, 16.0, viewport);
        let initial_pan = state.pan;

        // Egui registers the canvas interaction ID during a frame before it can receive a
        // subsequent pointer press, mirroring the native app's already-running canvas.
        let _ = ctx.run(input(2048), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_canvas(
                    ui,
                    &mut state,
                    Some(&image),
                    &RenderingConfig::default(),
                    false,
                    CanvasInput {
                        enabled: true,
                        wheel_zoom: false,
                    },
                    None,
                );
            });
        });

        let _ = ctx.run(
            input_with_events(
                2048,
                vec![
                    egui::Event::PointerMoved(start),
                    egui::Event::PointerButton {
                        pos: start,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            ),
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_canvas(
                        ui,
                        &mut state,
                        Some(&image),
                        &RenderingConfig::default(),
                        false,
                        CanvasInput {
                            enabled: true,
                            wheel_zoom: false,
                        },
                        None,
                    );
                });
            },
        );
        for position in [mid, end] {
            let _ = ctx.run(
                input_with_events(2048, vec![egui::Event::PointerMoved(position)]),
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        render_canvas(
                            ui,
                            &mut state,
                            Some(&image),
                            &RenderingConfig::default(),
                            false,
                            CanvasInput {
                                enabled: true,
                                wheel_zoom: false,
                            },
                            None,
                        );
                    });
                },
            );
        }

        assert!(
            state.pan.x > initial_pan.x,
            "dragging in the filmstrip wheel-only zone must still pan the canvas"
        );
    }

    #[test]
    fn completing_a_transition_does_not_stop_zoom_motion() {
        let ctx = egui::Context::default();
        let mut state = CanvasState::default();
        let first = Arc::new(decoded(2, 2));
        let second = Arc::new(decoded(2, 2));
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0));
        state.reset_view_actual_size(2.0, 2.0, viewport);
        let _ = ctx.run(input(2048), |ctx| {
            state
                .update_texture(ctx, &first, FilterMode::Auto, true, false)
                .expect("initial upload");
            state
                .update_texture(ctx, &second, FilterMode::Auto, true, true)
                .expect("transition upload");
        });
        state.target_scale = 2.0;

        let moving = state.animate(0.2, &RenderingConfig::default());
        assert!(
            moving,
            "zoom motion must continue after the crossfade completes"
        );
        assert!(state.outgoing.is_none());
    }
}
