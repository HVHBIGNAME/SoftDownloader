use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText, Vec2};

use super::app::{Page, SoftDownloaderApp};
use super::theme::{self, Icon};
use crate::storage::is_installed;

const NOTIFICATION_LIFETIME: Duration = Duration::from_secs(8);

#[derive(Clone)]
pub(super) struct Notification {
    pub text: String,
    pub shown_at: Instant,
    pub undo: Option<BTreeSet<String>>,
}

impl SoftDownloaderApp {
    pub(super) fn notify(&mut self, text: String) {
        self.sounds
            .play(self.settings.sound, super::sounds::Cue::Success);
        self.message = Some(Notification {
            text,
            shown_at: Instant::now(),
            undo: None,
        });
    }

    pub(super) fn save_preferences(&mut self) -> bool {
        match self.store.save_settings(&self.settings) {
            Ok(()) => true,
            Err(error) => {
                self.error = Some(format!("Не удалось сохранить настройки: {error:#}"));
                false
            }
        }
    }

    pub(super) fn toggle_favorite(&mut self, id: &str) {
        let previous = self.settings.favorites.clone();
        if !self.settings.favorites.remove(id) {
            self.settings.favorites.insert(id.to_owned());
        }
        if !self.save_preferences() {
            self.settings.favorites = previous;
        }
    }

    pub(super) fn review_selection(&mut self) {
        self.page = Page::Catalog;
        self.filter.show_selection();
        self.imported_selection = None;
        self.details = None;
    }

    pub(super) fn clear_selection(&mut self) {
        if self.queue_active || self.selected.is_empty() {
            return;
        }
        let previous = std::mem::take(&mut self.selected);
        self.message = Some(Notification {
            text: format!("Выбор очищен · {}", previous.len()),
            shown_at: Instant::now(),
            undo: Some(previous),
        });
    }

    fn undo_clear(&mut self) {
        if self.queue_active
            || self
                .message
                .as_ref()
                .is_none_or(|notice| notice.shown_at.elapsed() >= NOTIFICATION_LIFETIME)
        {
            return;
        }
        if let Some(previous) = self.message.take().and_then(|notice| notice.undo) {
            self.selected.extend(previous.into_iter().filter(|id| {
                self.document
                    .catalog
                    .package(id)
                    .is_some_and(|p| !is_installed(p, &self.effective_state))
            }));
        }
    }

    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.has_modal() || self.list_busy || egui::Popup::is_any_open(ctx) {
            return;
        }
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL, egui::Key::K)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::F)
        }) {
            if !matches!(self.page, Page::Installed | Page::Catalog) {
                self.show_catalog(None);
            }
            self.focus_search = true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
            self.request_program_list_import();
        }
        let editing_text = ctx
            .memory(|memory| memory.focused())
            .and_then(|id| egui::TextEdit::load_state(ctx, id))
            .is_some();
        if !editing_text && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Z)) {
            self.undo_clear();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            if self.details.take().is_none() {
                if self.page == Page::Catalog {
                    self.filter.query.clear();
                }
                if self.page == Page::Installed {
                    self.installed_query.clear();
                }
            }
            if let Some(id) = ctx.memory(|memory| memory.focused()) {
                ctx.memory_mut(|memory| memory.surrender_focus(id));
            }
        }
    }

    pub(super) fn toast(&mut self, ctx: &egui::Context) {
        let colors = theme::colors(ctx);
        if self.has_modal() || self.list_busy {
            return;
        }
        let Some(notice) = self.message.clone() else {
            return;
        };
        let Some(remaining) = NOTIFICATION_LIFETIME.checked_sub(notice.shown_at.elapsed()) else {
            self.message = None;
            return;
        };
        ctx.request_repaint_after(remaining);
        egui::Area::new(egui::Id::new("notification"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-24.0, -90.0))
            .show(ctx, |ui| {
                colors.card_frame().show(ui, |ui| {
                    ui.set_max_width(520.0);
                    ui.horizontal(|ui| {
                        theme::icon(ui, Icon::Check, colors.accent);
                        ui.label(RichText::new(notice.text).color(colors.accent).size(12.0));
                        if notice.undo.is_some()
                            && ui
                                .add_enabled(
                                    !self.queue_active,
                                    egui::Button::new("Вернуть").small(),
                                )
                                .on_hover_text("Вернуть очищенный выбор · Ctrl+Z вне поля ввода")
                                .clicked()
                        {
                            self.undo_clear();
                        }
                        if ui.small_button("×").clicked() {
                            self.message = None;
                        }
                    });
                });
            });
    }
}
