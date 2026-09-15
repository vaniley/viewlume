use crate::config::NavigationConfig;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct FolderNavigator {
    pub files: Vec<PathBuf>,
    pub current_index: usize,
    pub current_dir: Option<PathBuf>,
}

impl FolderNavigator {
    pub fn is_supported_image(path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tiff" | "tif" | "qoi"
            )
        } else {
            false
        }
    }

    pub fn scan_directory(&mut self, target_path: &Path) {
        let (dir, target_file) = if target_path.is_dir() {
            (target_path.to_path_buf(), None)
        } else {
            let parent = target_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
            (parent, Some(target_path.to_path_buf()))
        };

        self.current_dir = Some(dir.clone());
        self.files.clear();

        if let Ok(entries) = fs::read_dir(&dir) {
            let mut found_files: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|entry| entry.path()))
                .filter(|p| p.is_file() && Self::is_supported_image(p))
                .collect();

            // Natural sort order: image1, image2, image10
            found_files.sort_by(|a, b| {
                let name_a = a.file_name().unwrap_or_default().to_string_lossy();
                let name_b = b.file_name().unwrap_or_default().to_string_lossy();
                natord::compare(&name_a, &name_b)
            });

            self.files = found_files;
        }

        // Set current index
        if let Some(target) = target_file {
            let target_canon = fs::canonicalize(&target).unwrap_or(target);
            if let Some(idx) = self
                .files
                .iter()
                .position(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()) == target_canon)
            {
                self.current_index = idx;
            } else {
                self.current_index = 0;
            }
        } else {
            self.current_index = 0;
        }
    }

    pub fn current_path(&self) -> Option<&PathBuf> {
        self.files.get(self.current_index)
    }

    pub fn next(&mut self, wrap: bool) -> Option<usize> {
        if self.files.is_empty() {
            return None;
        }
        if self.current_index + 1 < self.files.len() {
            self.current_index += 1;
            Some(self.current_index)
        } else if wrap {
            self.current_index = 0;
            Some(self.current_index)
        } else {
            None
        }
    }

    pub fn prev(&mut self, wrap: bool) -> Option<usize> {
        if self.files.is_empty() {
            return None;
        }
        if self.current_index > 0 {
            self.current_index -= 1;
            Some(self.current_index)
        } else if wrap {
            self.current_index = self.files.len() - 1;
            Some(self.current_index)
        } else {
            None
        }
    }

    pub fn jump_to(&mut self, index: usize) -> Option<usize> {
        if index < self.files.len() {
            self.current_index = index;
            Some(self.current_index)
        } else {
            None
        }
    }

    pub fn total_count(&self) -> usize {
        self.files.len()
    }
}

pub enum NavAction {
    None,
    Prev,
    Next,
}

pub fn render_edge_chevrons(
    ui: &mut Ui,
    viewport: Rect,
    nav_cfg: &NavigationConfig,
    total_files: usize,
    opacity: f32,
) -> NavAction {
    if !nav_cfg.edge_buttons_enabled || total_files <= 1 || opacity <= 0.01 {
        return NavAction::None;
    }

    let edge_w = (viewport.width() * nav_cfg.edge_hover_width_ratio).clamp(60.0, 120.0);
    let top = viewport.min.y + 60.0;
    let height = (viewport.height() - 160.0).max(100.0);

    let left_rect = Rect::from_min_size(Pos2::new(viewport.min.x, top), Vec2::new(edge_w, height));
    let right_rect = Rect::from_min_size(
        Pos2::new(viewport.max.x - edge_w, top),
        Vec2::new(edge_w, height),
    );

    let mut action = NavAction::None;

    // Left chevron
    let left_resp = ui.allocate_rect(left_rect, Sense::click());
    let left_hover = if nav_cfg.animate_buttons {
        ui.ctx()
            .animate_bool_with_time(left_resp.id, left_resp.hovered(), 0.16)
    } else {
        f32::from(left_resp.hovered())
    };

    if left_resp.clicked() {
        action = NavAction::Prev;
    }

    // Right chevron
    let right_resp = ui.allocate_rect(right_rect, Sense::click());
    let right_hover = if nav_cfg.animate_buttons {
        ui.ctx()
            .animate_bool_with_time(right_resp.id, right_resp.hovered(), 0.16)
    } else {
        f32::from(right_resp.hovered())
    };

    if right_resp.clicked() {
        action = NavAction::Next;
    }

    let painter = ui.painter();

    // Draw left chevron button (<)
    let left_center = left_rect.center();
    let left_pressed = f32::from(left_resp.is_pointer_button_down_on());
    let left_alpha = ((44.0 + left_hover * 142.0 + left_pressed * 36.0) * opacity) as u8;
    let left_fg = ((112.0 + left_hover * 143.0) * opacity) as u8;
    let left_radius = 21.0 + left_hover * 3.0;
    let left_bg = Color32::from_rgba_premultiplied(22, 25, 32, left_alpha);
    painter.circle_filled(left_center, left_radius, left_bg);
    let left_stroke = Stroke::new(
        2.2_f32,
        Color32::from_rgba_premultiplied(255, 255, 255, left_fg),
    );
    let left_shift = -left_hover * 2.0;
    let l_p1 = Pos2::new(left_center.x + 4.0 + left_shift, left_center.y - 10.0);
    let l_p2 = Pos2::new(left_center.x - 4.0 + left_shift, left_center.y);
    let l_p3 = Pos2::new(left_center.x + 4.0 + left_shift, left_center.y + 10.0);
    painter.line_segment([l_p1, l_p2], left_stroke);
    painter.line_segment([l_p2, l_p3], left_stroke);

    // Draw right chevron button (>)
    let right_center = right_rect.center();
    let right_pressed = f32::from(right_resp.is_pointer_button_down_on());
    let right_alpha = ((44.0 + right_hover * 142.0 + right_pressed * 36.0) * opacity) as u8;
    let right_fg = ((112.0 + right_hover * 143.0) * opacity) as u8;
    let right_radius = 21.0 + right_hover * 3.0;
    let right_bg = Color32::from_rgba_premultiplied(22, 25, 32, right_alpha);
    painter.circle_filled(right_center, right_radius, right_bg);
    let right_stroke = Stroke::new(
        2.2_f32,
        Color32::from_rgba_premultiplied(255, 255, 255, right_fg),
    );
    let right_shift = right_hover * 2.0;
    let r_p1 = Pos2::new(right_center.x - 4.0 + right_shift, right_center.y - 10.0);
    let r_p2 = Pos2::new(right_center.x + 4.0 + right_shift, right_center.y);
    let r_p3 = Pos2::new(right_center.x - 4.0 + right_shift, right_center.y + 10.0);
    painter.line_segment([r_p1, r_p2], right_stroke);
    painter.line_segment([r_p2, r_p3], right_stroke);

    action
}
