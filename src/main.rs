#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod app;
pub mod cache;
pub mod canvas;
pub mod config;
pub mod diagnostics;
pub mod filmstrip;
pub mod loader;
pub mod navigation;
pub mod presentation;
pub mod settings_dialog;

use app::ImageViewerApp;
use config::{ViewerConfig, WindowMode};
use diagnostics::{record_renderer_state, record_startup_failure};
use presentation::PresentationPolicy;
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
    let presentation = PresentationPolicy::from_environment();
    let language = config.general.language;
    let is_overlay = config.window.mode == WindowMode::Overlay;

    let mut viewport_builder = egui::ViewportBuilder::default()
        .with_title("Viewlume")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([480.0, 320.0])
        // Transparency must be requested when the native window is created.
        // The explicit compatibility override instead creates a truly opaque window.
        .with_transparent(presentation.transparent_window())
        .with_drag_and_drop(true);

    if is_overlay {
        viewport_builder = viewport_builder
            .with_decorations(false)
            .with_fullscreen(true);
    } else {
        viewport_builder = viewport_builder.with_decorations(true).with_maximized(true);
    }

    let native_options = eframe::NativeOptions {
        viewport: viewport_builder,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    let result = eframe::run_native(
        "Viewlume",
        native_options,
        Box::new(move |cc| {
            record_renderer_state(cc, presentation);
            Ok(Box::new(ImageViewerApp::new(
                cc,
                initial_target,
                config,
                presentation,
            )))
        }),
    );

    if let Err(error) = &result {
        record_startup_failure(presentation, &error.to_string());
        show_startup_error(language, presentation);
    }

    result
}

fn show_startup_error(language: config::Language, presentation: PresentationPolicy) {
    let russian = language.is_russian();
    let description = if cfg!(target_os = "windows") && !presentation.opaque_window() {
        if russian {
            "Viewlume не удалось запустить GPU-рендерер. Подробности могут быть доступны в diagnostics.log рядом с config.toml.\n\nЕсли ошибка связана с прозрачным окном, запустите один раз с VIEWLUME_OPAQUE_WINDOW=1. Это может помочь с совместимостью композитора и прозрачности, но не исправляет недоступный адаптер или неподдерживаемый драйвер."
        } else {
            "Viewlume could not start its GPU renderer. Details may be available in diagnostics.log beside config.toml.\n\nIf this involves a transparent window, launch once with VIEWLUME_OPAQUE_WINDOW=1. It can help compositor/transparency compatibility, but cannot fix an unavailable adapter or unsupported driver."
        }
    } else if russian {
        "Viewlume не удалось запустить GPU-рендерер. Подробности могут быть доступны в diagnostics.log рядом с config.toml."
    } else {
        "Viewlume could not start its GPU renderer. Details may be available in diagnostics.log beside config.toml."
    };

    let title = if russian {
        "Ошибка запуска Viewlume"
    } else {
        "Viewlume startup error"
    };
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(description)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}
