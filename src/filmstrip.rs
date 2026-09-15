use egui::{Color32, Pos2, Rect, ScrollArea, Sense, TextureHandle, TextureOptions, Ui, Vec2};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use crate::config::{FilmstripConfig, FilmstripVisibility};
use crate::loader::pipeline::LoaderPipeline;

const MAX_THUMB_TEXTURES: usize = 512;

pub struct FilmstripState {
    pub thumb_textures: HashMap<PathBuf, TextureHandle>,
    thumb_lru: VecDeque<PathBuf>,
    pub hover_anim: f32,
    last_scrolled_index: Option<usize>,
    first_reveal: bool,
}

impl Default for FilmstripState {
    fn default() -> Self {
        Self {
            thumb_textures: HashMap::new(),
            thumb_lru: VecDeque::new(),
            hover_anim: 0.0,
            last_scrolled_index: None,
            first_reveal: true,
        }
    }
}

pub struct FilmstripResponse {
    pub selected_index: Option<usize>,
    pub scroll_nav: ScrollNav,
}

#[derive(Default)]
pub enum ScrollNav {
    #[default]
    None,
    Next,
    Prev,
}

#[allow(clippy::too_many_arguments)]
pub fn render_filmstrip(
    ui: &mut Ui,
    state: &mut FilmstripState,
    pipeline: &LoaderPipeline,
    files: &[PathBuf],
    current_index: usize,
    cfg: &FilmstripConfig,
    viewport: Rect,
    cursor_pos: Option<Pos2>,
    dt: f32,
) -> FilmstripResponse {
    let mut selected_index = None;
    let mut scroll_nav = ScrollNav::None;
    if files.is_empty() {
        return FilmstripResponse {
            selected_index,
            scroll_nav,
        };
    }

    let base_size = cfg.thumbnail_size as f32;
    let strip_height = (base_size + 36.0).clamp(72.0, 172.0);
    let cursor_in_zone = cursor_pos.is_some_and(|position| {
        position.y >= viewport.max.y - cfg.bottom_hover_height.max(strip_height + 12.0)
    });
    let target = match cfg.visibility {
        FilmstripVisibility::Always => 1.0,
        FilmstripVisibility::Hidden => 0.0,
        FilmstripVisibility::Hover => f32::from(cursor_in_zone),
    };
    let rate = if target > state.hover_anim {
        13.0
    } else {
        19.0
    };
    state.hover_anim += (target - state.hover_anim) * (rate * dt).min(1.0);
    if (target - state.hover_anim).abs() > 0.01 {
        ui.ctx().request_repaint();
    }
    if state.hover_anim <= 0.01 {
        return FilmstripResponse {
            selected_index,
            scroll_nav,
        };
    }

    let slide = (1.0 - state.hover_anim) * (strip_height + 8.0);
    let strip_rect = Rect::from_min_size(
        Pos2::new(viewport.min.x, viewport.max.y - strip_height + slide),
        Vec2::new(viewport.width(), strip_height),
    );
    let strip_response = ui.allocate_rect(strip_rect, Sense::hover());
    let wheel_delta = ui.input(|i| i.raw_scroll_delta.y);
    if strip_response.hovered() && wheel_delta.abs() > 1.0 {
        if wheel_delta < 0.0 {
            scroll_nav = ScrollNav::Next;
        } else {
            scroll_nav = ScrollNav::Prev;
        }
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(strip_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.style_mut().always_scroll_the_only_direction = true;

    let gap = 10.0;
    let stride = base_size + gap;
    // Side padding lets the first and last thumbnails reach the visual center.
    let side_padding = (strip_rect.width() * 0.5 - base_size * 0.5).max(0.0);
    let total_width = side_padding * 2.0 + files.len() as f32 * stride - gap;

    // Pre-request thumbnails around the active image so they're already loading
    // (or cached) before the carousel viewport renders them.
    let prefetch_radius = ((strip_rect.width() / stride).ceil() as usize).max(8);
    let pre_first = current_index.saturating_sub(prefetch_radius);
    let pre_last = (current_index + prefetch_radius + 1).min(files.len());
    for (idx, path) in files.iter().enumerate().take(pre_last).skip(pre_first) {
        pipeline.request_thumbnail(path.clone(), idx, cfg.thumbnail_size);
    }

    // On first reveal, disable scroll animation so the carousel appears
    // already centered on the active thumbnail instead of visibly scrolling.
    let use_animated = !state.first_reveal;

    ScrollArea::horizontal()
        .id_salt("filmstrip-scroll")
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .animated(use_animated)
        .show_viewport(&mut child, |ui, visible| {
            ui.set_width(total_width.max(visible.width()));
            ui.set_height(strip_height);
            let origin = ui.max_rect().left_top();

            if cfg.auto_scroll_to_active && state.last_scrolled_index != Some(current_index) {
                let target_rect = Rect::from_min_size(
                    Pos2::new(
                        origin.x + side_padding + current_index as f32 * stride,
                        origin.y,
                    ),
                    Vec2::new(base_size, strip_height),
                );
                ui.scroll_to_rect(target_rect, Some(egui::Align::Center));
                state.last_scrolled_index = Some(current_index);
                state.first_reveal = false;
            }

            // `visible` is content-relative. Mixing it with absolute coordinates caused
            // thumbnails to disappear after scrolling in the previous implementation.
            let first =
                (((visible.min.x - side_padding) / stride).floor() as isize - 3).max(0) as usize;
            let last =
                ((((visible.max.x - side_padding) / stride).ceil() as usize) + 3).min(files.len());
            let center_x = visible.center().x;
            let influence = (visible.width() * 0.52).max(base_size * 2.0);

            let mut indices: Vec<usize> = (first..last).collect();
            // Paint the center item last so the larger image naturally sits above its neighbors.
            indices.sort_by(|a, b| {
                let ax = side_padding + *a as f32 * stride + base_size * 0.5;
                let bx = side_padding + *b as f32 * stride + base_size * 0.5;
                (bx - center_x).abs().total_cmp(&(ax - center_x).abs())
            });

            for idx in indices {
                let path = &files[idx];
                let slot_center_x = side_padding + idx as f32 * stride + base_size * 0.5;
                let proximity =
                    (1.0 - (slot_center_x - center_x).abs() / influence).clamp(0.0, 1.0);
                let eased = proximity * proximity * (3.0 - 2.0 * proximity);
                let scale = if cfg.coverflow_effect {
                    0.45 + eased * eased * 0.55
                } else {
                    1.0
                };
                let draw_size = base_size * scale;
                let drop_offset = if cfg.coverflow_effect {
                    (1.0 - scale) * base_size * 0.4
                } else {
                    0.0
                };
                let center = Pos2::new(
                    origin.x + slot_center_x,
                    origin.y + strip_height * 0.5 - 3.0 + drop_offset,
                );
                let hit_rect = Rect::from_center_size(center, Vec2::splat(base_size));
                let mut response = ui.interact(hit_rect, ui.id().with(idx), Sense::click());
                if response.clicked() {
                    selected_index = Some(idx);
                }
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    response = response.on_hover_text(name);
                }

                let texture_id = if let Some(texture) = state.thumb_textures.get(path) {
                    let result = (texture.id(), texture.size_vec2());
                    state.touch_lru(path);
                    Some(result)
                } else if let Some(image) =
                    pipeline.request_thumbnail(path.clone(), idx, cfg.thumbnail_size)
                {
                    let texture = ui.ctx().load_texture(
                        format!("thumb-{idx}"),
                        image.clone(),
                        TextureOptions::LINEAR,
                    );
                    let result = (texture.id(), texture.size_vec2());
                    state.insert_thumb_texture(path.clone(), texture);
                    Some(result)
                } else {
                    None
                };

                if let Some((texture_id, texture_size)) = texture_id {
                    let fit = (draw_size / texture_size.x).min(draw_size / texture_size.y);
                    let image_size = texture_size * fit;
                    let image_rect = Rect::from_center_size(center, image_size);
                    let alpha = (100.0 + 155.0 * eased) as u8;
                    ui.painter().image(
                        texture_id,
                        image_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::from_white_alpha(alpha),
                    );
                } else {
                    ui.painter().rect_filled(
                        Rect::from_center_size(center, Vec2::splat(draw_size * 0.82)),
                        6.0,
                        Color32::from_rgba_premultiplied(150, 158, 174, 32),
                    );
                }

                if idx == current_index {
                    ui.painter().circle_filled(
                        Pos2::new(center.x, origin.y + strip_height - 9.0),
                        2.5,
                        Color32::from_rgb(96, 165, 250),
                    );
                } else if response.hovered() {
                    ui.ctx().request_repaint();
                }
            }
        });

    FilmstripResponse {
        selected_index,
        scroll_nav,
    }
}

impl FilmstripState {
    fn touch_lru(&mut self, path: &PathBuf) {
        if let Some(pos) = self.thumb_lru.iter().position(|p| p == path) {
            self.thumb_lru.remove(pos);
        }
        self.thumb_lru.push_back(path.clone());
    }

    fn evict_if_needed(&mut self) {
        while self.thumb_textures.len() > MAX_THUMB_TEXTURES {
            if let Some(old) = self.thumb_lru.pop_front() {
                self.thumb_textures.remove(&old);
            } else {
                break;
            }
        }
    }

    pub fn insert_loaded_thumbnail(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        index: usize,
        thumb: Arc<egui::ColorImage>,
    ) {
        let texture = ctx.load_texture(format!("thumb-{index}"), thumb, TextureOptions::LINEAR);
        self.touch_lru(&path);
        self.thumb_textures.insert(path, texture);
        self.evict_if_needed();
    }

    pub fn insert_thumb_texture(&mut self, path: PathBuf, texture: TextureHandle) {
        self.touch_lru(&path);
        self.thumb_textures.insert(path, texture);
        self.evict_if_needed();
    }

    pub fn clear(&mut self) {
        self.thumb_textures.clear();
        self.thumb_lru.clear();
        self.last_scrolled_index = None;
        self.first_reveal = true;
    }
}
