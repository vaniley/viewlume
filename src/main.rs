#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod app;
pub mod cache;
pub mod canvas;
pub mod config;
pub mod filmstrip;
pub mod loader;
pub mod navigation;
pub mod settings_dialog;

use app::ImageViewerApp;
use config::{ViewerConfig, WindowMode};
use std::env;
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = env::args().collect();
    let initial_target = if args.len() > 1 {
        let path = PathBuf::from(&args[1]);
        if path.exists() {
            Some(path)
        } else {
            eprintln!("Warning: Target path does not exist: {:?}", path);
            None
        }
    } else {
        None
    };

    let config = ViewerConfig::load();
    let is_overlay = config.window.mode == WindowMode::Overlay;

    let viewport_builder = egui::ViewportBuilder::default()
        .with_title("Viewlume")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([480.0, 320.0])
        .with_resizable(true)
        // Windows fullscreen already masks the caption and resize frame. Keep
        // their native style enabled so leaving fullscreen restores both.
        .with_decorations(cfg!(windows) || !is_overlay)
        .with_fullscreen(is_overlay)
        .with_transparent(true)
        .with_drag_and_drop(true);

    let native_options = eframe::NativeOptions {
        viewport: viewport_builder,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "Viewlume",
        native_options,
        Box::new(move |cc| Ok(Box::new(ImageViewerApp::new(cc, initial_target, config)))),
    )
}
