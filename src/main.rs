#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use anyhow::{Context, Result};
use eframe::egui;
use softdownloader::{
    storage::Store,
    ui::{SoftDownloaderApp, theme},
};

fn main() {
    if let Err(error) = run() {
        let message = format!("{error:#}");
        eprintln!("SoftDownloader: {message}");
        rfd::MessageDialog::new()
            .set_title("SoftDownloader — ошибка запуска")
            .set_description(&message)
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let mut catalog = None;
    let mut data_dir = None;
    let mut update_health = None;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--apply-update") => {
                let directory = PathBuf::from(arguments.next().context("Нет папки обновления")?);
                anyhow::ensure!(arguments.next().is_none(), "Лишние параметры обновления");
                // This is the temporary copy of the old EXE, not the application window.
                let result = softdownloader::updater::apply_update(&directory);
                std::process::exit(if result.is_ok() { 0 } else { 1 });
            }
            Some("--update-health") => {
                update_health = Some(PathBuf::from(
                    arguments.next().context("Нет папки проверки запуска")?,
                ));
            }
            Some("--catalog") => {
                catalog = Some(
                    arguments
                        .next()
                        .context("После --catalog укажите ссылку или путь")?
                        .to_string_lossy()
                        .into_owned(),
                )
            }
            Some("--data-dir") => {
                data_dir = Some(PathBuf::from(
                    arguments.next().context("После --data-dir укажите папку")?,
                ))
            }
            Some("--help" | "-h") => {
                println!(
                    "SoftDownloader [--catalog <HTTPS URL or catalog.json>] [--data-dir <directory>]"
                );
                return Ok(());
            }
            _ => anyhow::bail!("Неизвестный параметр: {}", argument.to_string_lossy()),
        }
    }
    let store = match data_dir {
        Some(path) => Store::at(path)?,
        None => Store::open()?,
    };
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SoftDownloader")
            .with_app_id("SoftDownloader")
            .with_window_level(egui::WindowLevel::Normal)
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([1024.0, 700.0])
            .with_icon(theme::window_icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "SoftDownloader",
        native_options,
        Box::new(move |cc| {
            let app = SoftDownloaderApp::new(cc, store, catalog)?;
            if let Some(directory) = update_health {
                softdownloader::updater::acknowledge(&directory)?;
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}
