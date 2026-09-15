use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowMode {
    #[serde(rename = "overlay")]
    Overlay,
    #[serde(rename = "windowed")]
    Windowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DoubleClickAction {
    #[serde(rename = "toggle_mode")]
    ToggleWindowMode,
    #[serde(rename = "toggle_fit")]
    ToggleFitToWindow,
    #[serde(rename = "toggle_actual_size")]
    ToggleActualSize,
    #[serde(rename = "none")]
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterMode {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "nearest")]
    Nearest,
    #[serde(rename = "bilinear")]
    Bilinear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilmstripVisibility {
    #[serde(rename = "hover")]
    Hover,
    #[serde(rename = "always")]
    Always,
    #[serde(rename = "hidden")]
    Hidden,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub mode: WindowMode,
    pub double_click_action: DoubleClickAction,
    pub bg_color: [u8; 4],
    pub show_checkerboard_for_transparent: bool,
    pub auto_hide_ui: bool,
    pub ui_fade_timeout_secs: f32,
    pub remember_window_size: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            mode: WindowMode::Overlay,
            double_click_action: DoubleClickAction::ToggleWindowMode,
            bg_color: [14, 14, 16, 180], // Dark overlay, semi-transparent in overlay mode
            show_checkerboard_for_transparent: true,
            auto_hide_ui: true,
            ui_fade_timeout_secs: 2.0,
            remember_window_size: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderingConfig {
    pub filter_mode: FilterMode,
    pub zoom_factor: f64,
    pub min_zoom: f64,
    pub max_zoom: f64,
    pub reverse_zoom_direction: bool,
    pub zoom_towards_cursor: bool,
    pub smooth_pan: bool,
    pub animate_image_transitions: bool,
    pub image_transition_ms: f32,
}

impl Default for RenderingConfig {
    fn default() -> Self {
        Self {
            filter_mode: FilterMode::Auto,
            zoom_factor: 1.15,
            min_zoom: 0.002,
            max_zoom: 1500.0,
            reverse_zoom_direction: false,
            zoom_towards_cursor: true,
            smooth_pan: true,
            animate_image_transitions: true,
            image_transition_ms: 180.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NavigationConfig {
    pub loop_folder: bool,
    pub edge_buttons_enabled: bool,
    pub edge_hover_width_ratio: f32,
    pub animate_buttons: bool,
}

impl Default for NavigationConfig {
    fn default() -> Self {
        Self {
            loop_folder: true,
            edge_buttons_enabled: true,
            edge_hover_width_ratio: 0.08, // 8% of screen on left and right
            animate_buttons: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FilmstripConfig {
    pub visibility: FilmstripVisibility,
    pub thumbnail_size: u32,
    pub bottom_hover_height: f32,
    pub auto_scroll_to_active: bool,
    pub coverflow_effect: bool,
}

impl Default for FilmstripConfig {
    fn default() -> Self {
        Self {
            visibility: FilmstripVisibility::Hover,
            thumbnail_size: 72,
            bottom_hover_height: 96.0,
            auto_scroll_to_active: true,
            coverflow_effect: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    pub max_ram_cache_mb: usize,
    pub prefetch_count: usize,
    pub thumbnail_cache_count: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_ram_cache_mb: 1024,
            prefetch_count: 2,
            thumbnail_cache_count: 2000,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewerConfig {
    pub window: WindowConfig,
    pub rendering: RenderingConfig,
    pub navigation: NavigationConfig,
    pub filmstrip: FilmstripConfig,
    pub cache: CacheConfig,
}

impl ViewerConfig {
    pub fn config_path() -> PathBuf {
        if let Some(proj_dirs) = directories::ProjectDirs::from("io", "vaniley", "viewlume") {
            let config_dir = proj_dirs.config_dir();
            let _ = fs::create_dir_all(config_dir);
            config_dir.join("config.toml")
        } else {
            PathBuf::from("config.toml")
        }
    }

    fn legacy_config_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "image_viewer", "image-viewer")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    pub fn load() -> Self {
        let new_path = Self::config_path();
        let mut candidates = vec![new_path.clone()];
        if let Some(legacy_path) = Self::legacy_config_path() {
            if legacy_path != new_path {
                candidates.push(legacy_path);
            }
        }

        for path in candidates {
            if let Ok(content) = fs::read_to_string(&path) {
                match toml::from_str::<ViewerConfig>(&content) {
                    Ok(cfg) => {
                        if path != new_path {
                            let _ = cfg.save();
                        }
                        return cfg;
                    }
                    Err(err) => {
                        eprintln!(
                            "Warning: Failed to parse config file at {:?}: {}. Using defaults.",
                            path, err
                        );
                    }
                }
            }
        }
        let default_cfg = ViewerConfig::default();
        let _ = default_cfg.save();
        default_cfg
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let toml_str = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        fs::write(&path, toml_str)
            .map_err(|e| format!("Failed to write config to {:?}: {}", path, e))?;
        Ok(())
    }
}
