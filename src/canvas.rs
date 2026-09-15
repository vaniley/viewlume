use egui::{Color32, Pos2, Rect, Sense, TextureHandle, TextureOptions, Ui, Vec2};
use std::sync::Arc;

use crate::config::{FilterMode, RenderingConfig};
use crate::loader::decoder::DecodedImage;

struct OutgoingImage {
    texture: TextureHandle,
    rect: Rect,
}

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

    pub fn get_texture_options(filter_mode: FilterMode, scale: f64) -> TextureOptions {
        match filter_mode {
            FilterMode::Nearest => TextureOptions::NEAREST,
            FilterMode::Bilinear => TextureOptions::LINEAR,
            FilterMode::Auto if scale >= 1.0 => TextureOptions {
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Linear,
                wrap_mode: egui::TextureWrapMode::ClampToEdge,
                mipmap_mode: None,
            },
            FilterMode::Auto => TextureOptions::LINEAR,
        }
    }

    pub fn update_texture(
        &mut self,
        ctx: &egui::Context,
        image: &Arc<DecodedImage>,
        filter_mode: FilterMode,
        is_new_image: bool,
        animate: bool,
    ) {
        let options = Self::get_texture_options(filter_mode, self.target_scale);
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
            self.texture_sequence += 1;
            self.texture = Some(ctx.load_texture(
                format!("active-image-{}", self.texture_sequence),
                image.color_image.clone(),
                options,
            ));
            self.initialized = false;
        } else if let Some(texture) = self.texture.as_mut() {
            texture.set(image.color_image.clone(), options);
            self.outgoing = None;
            self.transition_progress = 1.0;
        } else {
            self.texture_sequence += 1;
            self.texture = Some(ctx.load_texture(
                format!("active-image-{}", self.texture_sequence),
                image.color_image.clone(),
                options,
            ));
        }
        self.current_filter = filter_mode;
    }

    pub fn sync_texture_options(&mut self, image: Option<&Arc<DecodedImage>>) {
        if self.current_filter == FilterMode::Auto {
            let options = Self::get_texture_options(self.current_filter, self.target_scale);
            if let Some(texture) = self.texture.as_mut() {
                if let Some(image) = image {
                    texture.set(image.color_image.clone(), options);
                }
            }
        }
    }

    pub fn update_animation_frame(&mut self, ctx: &egui::Context, frame: Arc<egui::ColorImage>) {
        let options = Self::get_texture_options(self.current_filter, self.target_scale);
        if let Some(texture) = self.texture.as_mut() {
            texture.set(frame, options);
        } else {
            self.texture_sequence += 1;
            self.texture = Some(ctx.load_texture(
                format!("active-image-{}", self.texture_sequence),
                frame,
                options,
            ));
        }
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
                moving = self.transition_progress < 1.0;
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

pub fn render_canvas(
    ui: &mut Ui,
    state: &mut CanvasState,
    current_image: Option<&Arc<DecodedImage>>,
    render_cfg: &RenderingConfig,
    show_checkerboard: bool,
    allow_wheel_zoom: bool,
    empty_hint: Option<&str>,
) -> CanvasResponse {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let cursor_pos = ui.input(|i| i.pointer.hover_pos());
    let dt = ui.input(|i| i.unstable_dt).clamp(1.0 / 240.0, 1.0 / 20.0);

    if let Some(image) = current_image {
        if !state.initialized {
            state.initialize_image(image.width as f32, image.height as f32, rect, render_cfg);
            state.sync_texture_options(current_image);
        }
    }

    let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
    if scroll_delta != 0.0 && response.hovered() && allow_wheel_zoom {
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
            state.sync_texture_options(current_image);
        }
    }

    let dragging = response.dragged_by(egui::PointerButton::Primary)
        || response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle);
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
        if let Some(texture) = state.texture.as_ref() {
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
                let cols = ((intersect.width() / cell_size).ceil() as usize).min(120);
                let rows = ((intersect.height() / cell_size).ceil() as usize).min(120);
                for row in 0..rows {
                    for col in 0..cols {
                        let min = Pos2::new(
                            intersect.min.x + col as f32 * cell_size,
                            intersect.min.y + row as f32 * cell_size,
                        );
                        let max = Pos2::new(
                            (min.x + cell_size).min(intersect.max.x),
                            (min.y + cell_size).min(intersect.max.y),
                        );
                        let color = if (row + col) % 2 == 0 {
                            Color32::from_gray(32)
                        } else {
                            Color32::from_gray(22)
                        };
                        painter.rect_filled(Rect::from_min_max(min, max), 0.0, color);
                    }
                }
            }

            painter.image(
                texture.id(),
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
        double_clicked: response.double_clicked(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(width: u32, height: u32) -> DecodedImage {
        DecodedImage {
            width,
            height,
            bytes_size: (width * height * 4) as usize,
            color_image: Arc::new(egui::ColorImage::new(
                [width as usize, height as usize],
                Color32::WHITE,
            )),
            animation_frames: Arc::from([]),
        }
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

        let opts_scaled_down = CanvasState::get_texture_options(FilterMode::Auto, 0.5);
        assert_eq!(opts_scaled_down.magnification, egui::TextureFilter::Linear);
    }
}
