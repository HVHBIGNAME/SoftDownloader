use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

use super::app::SoftDownloaderApp;
use super::theme;
use crate::storage::Store;
use crate::updater::{self, Handoff, ReadyUpdate};

pub(super) struct UpdateUi {
    pub busy: bool,
    pub progress: Option<crate::transfer::Progress>,
    message: String,
    ready: Option<ReadyUpdate>,
    next_check: Instant,
    idle_since: Option<Instant>,
    deferred: bool,
    handoff: Option<Handoff>,
    restarting: bool,
}

impl UpdateUi {
    pub fn new(store: &Store) -> Self {
        let message = match updater::previous_result(store) {
            Ok(result) if !result.message.is_empty() => result.message,
            Ok(_) => "Проверка обновлений при запуске и каждые 6 часов".into(),
            Err(error) => format!("Не удалось прочитать результат обновления: {error:#}"),
        };
        Self {
            busy: false,
            progress: None,
            message,
            ready: None,
            next_check: Instant::now() + Duration::from_secs(30),
            idle_since: None,
            deferred: false,
            handoff: None,
            restarting: false,
        }
    }

    pub fn applying(&self) -> bool {
        self.handoff.is_some() || self.restarting
    }
}

fn restart_due(idle_since: &mut Option<Instant>, idle: bool, now: Instant) -> bool {
    if !idle {
        *idle_since = None;
        return false;
    }
    now.duration_since(*idle_since.get_or_insert(now)) >= Duration::from_secs(15)
}

impl SoftDownloaderApp {
    fn check_app_update(&mut self) {
        if self.updater.busy || self.updater.applying() {
            return;
        }
        self.updater.ready = None;
        self.updater.deferred = false;
        self.updater.idle_since = None;
        self.updater.progress = None;
        self.updater.busy = true;
        self.updater.message = "Проверка GitHub Releases…".into();
        self.engine.check_app_update(self.store.clone());
    }

    pub(super) fn receive_app_update(&mut self, result: Result<Option<ReadyUpdate>, String>) {
        self.updater.busy = false;
        self.updater.progress = None;
        self.updater.next_check = Instant::now() + Duration::from_secs(6 * 60 * 60);
        if !self.settings.updates.enabled {
            self.updater.message = "Автообновление выключено".into();
            return;
        }
        match result {
            Ok(Some(update)) => {
                self.updater.message = format!("Версия {} скачана и проверена", update.version);
                self.updater.ready = Some(update);
            }
            Ok(None) => self.updater.message = "Установлена актуальная версия".into(),
            Err(error) => self.updater.message = format!("Обновление не выполнено: {error}"),
        }
    }

    pub(super) fn tick_app_update(&mut self, ctx: &egui::Context) {
        if self.updater.restarting {
            return;
        }
        if let Some(handoff) = &mut self.updater.handoff {
            match handoff.ready() {
                Ok(true) => {
                    self.updater.restarting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Ok(false) => ctx.request_repaint_after(Duration::from_millis(100)),
                Err(error) => {
                    self.updater.message = format!("Обновление не выполнено: {error:#}");
                    self.updater.handoff = None;
                }
            }
            return;
        }
        if !self.settings.updates.enabled || !cfg!(windows) || cfg!(debug_assertions) {
            return;
        }
        if !self.updater.busy
            && self.updater.ready.is_none()
            && Instant::now() >= self.updater.next_check
        {
            self.check_app_update();
        }
        if !self.updater.busy && self.updater.ready.is_none() {
            ctx.request_repaint_after(
                self.updater
                    .next_check
                    .saturating_duration_since(Instant::now()),
            );
        }
        if self.updater.ready.is_none() || self.updater.deferred {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        let idle = !self.queue_active
            && !self.loading
            && !self.programs_loading
            && !self.list_busy
            && !self.background_busy
            && !self.has_modal()
            && !egui::Popup::is_any_open(ctx)
            && !ctx.input(|i| i.viewport().close_requested());
        if !restart_due(&mut self.updater.idle_since, idle, Instant::now()) {
            return;
        }
        self.settings.restore_selection = self.selected.clone();
        if !self.save_preferences() {
            self.updater.deferred = true;
            return;
        }
        let Some(update) = self.updater.ready.take() else {
            return;
        };
        match updater::launch(update, &self.store, &self.active_source) {
            Ok(handoff) => {
                self.updater.message = "Замена EXE и перезапуск…".into();
                self.updater.handoff = Some(handoff);
            }
            Err(error) => self.updater.message = format!("Обновление не выполнено: {error:#}"),
        }
    }

    pub(super) fn update_banner(&mut self, ctx: &egui::Context) {
        if self.updater.ready.is_none() || self.updater.deferred {
            return;
        }
        let colors = theme::colors(ctx);
        egui::TopBottomPanel::top("app-update-banner").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&self.updater.message).color(colors.accent));
                let text = self.updater.idle_since.map_or_else(
                    || "Перезапуск после завершения текущих операций".into(),
                    |since| {
                        format!(
                            "Автоматический перезапуск через {} с",
                            15_u64.saturating_sub(since.elapsed().as_secs())
                        )
                    },
                );
                ui.label(text);
                if ui.button("Позже").clicked() {
                    self.updater.deferred = true;
                }
            });
        });
    }

    pub(super) fn update_settings(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        colors.card_frame().show(ui, |ui| {
            ui.label(RichText::new(format!("SoftDownloader {}", env!("CARGO_PKG_VERSION"))).size(20.0).strong());
            let was_enabled = self.settings.updates.enabled;
            if ui.add_enabled(!self.updater.applying(), egui::Checkbox::new(&mut self.settings.updates.enabled, "Обновлять автоматически из GitHub Releases")).changed() {
                if self.save_preferences() {
                    if self.settings.updates.enabled { self.updater.next_check = Instant::now(); }
                    else {
                        self.engine.cancel_app_update();
                        self.updater.ready = None;
                        self.updater.message = "Автообновление выключено".into();
                    }
                } else { self.settings.updates.enabled = was_enabled; }
            }
            ui.label("Новая стабильная версия скачивается в фоне, проверяется по SHA-256 и применяется после завершения операций. Приложение перезапустится автоматически, настройки и выбор сохранятся.");
            ui.label("Для замены EXE нужна запись в его папку. При неудачном запуске возвращается прежняя версия.");
            ui.add_space(8.0);
            ui.label(&self.updater.message);
            if let Some(progress) = &self.updater.progress {
                ui.add(egui::ProgressBar::new(progress.downloaded as f32 / progress.total.max(1) as f32).show_percentage());
            }
            if cfg!(debug_assertions) { ui.label("В отладочной сборке автоматическая замена EXE отключена."); }
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(self.settings.updates.enabled && !self.updater.busy && !self.updater.applying(), egui::Button::new("Проверить сейчас")).clicked() { self.check_app_update(); }
                if self.updater.deferred && self.updater.ready.is_some() && ui.button("Продолжить обновление").clicked() {
                    self.updater.deferred = false;
                    self.updater.idle_since = None;
                }
                ui.hyperlink_to("Релизы на GitHub", updater::RELEASES_URL);
                if ui.button("Что нового").clicked() { self.experience.news(); }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_countdown_requires_uninterrupted_idle_time() {
        let start = Instant::now();
        let mut idle = None;
        assert!(!restart_due(&mut idle, true, start));
        assert!(!restart_due(
            &mut idle,
            false,
            start + Duration::from_secs(20)
        ));
        assert!(!restart_due(
            &mut idle,
            true,
            start + Duration::from_secs(21)
        ));
        assert!(!restart_due(
            &mut idle,
            true,
            start + Duration::from_secs(35)
        ));
        assert!(restart_due(
            &mut idle,
            true,
            start + Duration::from_secs(36)
        ));
    }
}
