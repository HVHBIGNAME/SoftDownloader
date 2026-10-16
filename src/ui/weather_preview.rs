use eframe::egui;
use serde::Deserialize;

use super::app::{Page, SoftDownloaderApp};
use crate::{
    storage::Store,
    weather::{Conditions, WeatherKind},
};

#[derive(Deserialize)]
struct Preview {
    kind: WeatherKind,
    #[serde(default)]
    bare: bool,
    #[serde(default)]
    cycle: bool,
}

struct PreviewApp {
    app: SoftDownloaderApp,
    bare: bool,
    cycle: bool,
    started: std::time::Instant,
}

impl eframe::App for PreviewApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.background.observe_cpu(frame.info().cpu_usage);
        if self.cycle {
            use crate::weather::Lighting;
            let step = (self.started.elapsed().as_secs() / 5) as usize;
            self.app.settings.weather.lighting = [
                Lighting::Day,
                Lighting::Evening,
                Lighting::Dusk,
                Lighting::Night,
                Lighting::Dawn,
            ][step % 5];
            let kind = [
                WeatherKind::Rain,
                WeatherKind::Downpour,
                WeatherKind::Hail,
                WeatherKind::Snow,
                WeatherKind::Blizzard,
                WeatherKind::Storm,
                WeatherKind::Clear,
            ][step % 7];
            self.app.weather.preview = Some(Conditions::demo(kind));
        }
        if !self.bare {
            self.app.render(ctx);
            return;
        }
        let (settings, scene) = self.app.weather_background();
        self.app
            .background
            .update(ctx, &settings, self.app.settings.reduced_motion, scene);
        if let Some(scene) = scene {
            self.app.background.paint_weather(
                &ctx.layer_painter(egui::LayerId::background()),
                ctx.content_rect(),
                scene,
            );
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
}

pub fn run_weather_preview(store: Store) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([1024.0, 700.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "SoftDownloader",
        options,
        Box::new(move |cc| {
            let config: Preview =
                serde_json::from_slice(&std::fs::read(store.root.join("preview.json"))?)?;
            let mut app = SoftDownloaderApp::from_context(&cc.egui_ctx, store, None)?;
            app.settings.updates.enabled = false;
            app.settings.sound.volume = 0;
            app.settings.weather.enabled = false;
            app.settings.weather.cinematic = false;
            app.weather.preview = Some(Conditions::demo(config.kind));
            app.page = Page::Weather;
            app.catalog_status = "Предпросмотр · без сети".into();
            Ok(Box::new(PreviewApp {
                app,
                bare: config.bare,
                cycle: config.cycle,
                started: std::time::Instant::now(),
            }))
        }),
    )
}
