use eframe::egui::{self, RichText};

use super::{app::SoftDownloaderApp, theme};

use crate::preferences::ExperienceSettings;

pub(super) struct ExperienceUi {
    guide: Option<usize>,
    news: bool,
}

impl ExperienceUi {
    pub fn new(settings: &ExperienceSettings, existing_profile: bool) -> Self {
        Self {
            guide: (!settings.guide_seen).then_some(0),
            news: existing_profile && settings.last_seen_version != env!("CARGO_PKG_VERSION"),
        }
    }
    pub fn open(&self) -> bool {
        self.guide.is_some() || self.news
    }
    pub fn guide(&mut self) {
        self.guide = Some(0);
    }
    pub fn news(&mut self) {
        self.news = true;
    }
}

const STEPS: &[(&str, &str, &str)] = &[
    (
        "Добро пожаловать",
        "Весь нужный софт — в одном месте",
        "Найдите программу по названию, издателю или тегу. Категории и избранное помогут собрать свою коллекцию. Ctrl+K открывает поиск.",
    ),
    (
        "Ваш набор",
        "Один компьютер — один удобный список",
        "Отмечайте программы галочками или выберите готовый набор. Сохраните собственную подборку и перенесите её на другой ПК через JSON.",
    ),
    (
        "Установка",
        "Вы управляете каждым запуском",
        "«Установить» запускает очередь сразу. «План» показывает зависимости заранее. Выберите тихий режим или мастер; обязательное подтверждение включается в настройках.",
    ),
    (
        "Погода",
        "Атмосфера вашего города",
        "Откройте «Погода», начните вводить город и выберите его из списка. Можно определить примерный город по IP. Погодные видеосцены загружаются по необходимости.",
    ),
    (
        "Сделайте по-своему",
        "Темы, звук и помощь всегда рядом",
        "В настройках доступны цветовые темы, сезонные сцены и новые звуковые наборы. Гайд можно открыть снова кнопкой «Помощь». После обновления здесь появится краткое «Что нового».",
    ),
];

const CHANGES: &[(&str, &str)] = &[
    (
        "Погода",
        "Поиск города, выбор по IP, автоматическое обновление, дождь, снег, метель и туман на фоне.",
    ),
    (
        "Атмосфера",
        "Настоящие видеосцены, мягкий объёмный снег и новое хэллоуинское оформление.",
    ),
    (
        "Звуки",
        "Новые музыкальные сигналы, отдельные события и прослушивание каждого набора.",
    ),
    (
        "Первый запуск",
        "Пошаговый гайд с сохранением прохождения и возможность открыть его заново.",
    ),
    ("Каталог", "Больше полезных приложений и готовых наборов."),
];

impl SoftDownloaderApp {
    pub(super) fn experience_dialogs(&mut self, ctx: &egui::Context) {
        if self.queue_active
            || self.confirm_plan.is_some()
            || self.confirm_removal.is_some()
            || self.import_preview.is_some()
            || self.updater.applying()
        {
            return;
        }
        if let Some(step) = self.experience.guide {
            self.guide_dialog(ctx, step);
        } else if self.experience.news {
            self.news_dialog(ctx);
        }
    }

    fn guide_dialog(&mut self, ctx: &egui::Context, step: usize) {
        let colors = theme::colors(ctx);
        let (title, subtitle, body) = STEPS[step];
        let mut done = false;
        let response = egui::Modal::new(egui::Id::new("welcome-guide"))
            .frame(colors.dialog_frame().inner_margin(26))
            .show(ctx, |ui| {
                ui.set_width(520.0);
                ui.horizontal(|ui| {
                    theme::logo(ui, 42.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("SoftDownloader").size(20.0).strong());
                        ui.label(format!("Знакомство · {} из {}", step + 1, STEPS.len()));
                    });
                });
                ui.add_space(12.0);
                ui.add(
                    egui::ProgressBar::new((step + 1) as f32 / STEPS.len() as f32)
                        .desired_height(4.0),
                );
                ui.add_space(18.0);
                theme::heading(ui, title, subtitle);
                ui.label(RichText::new(body).size(15.0));
                ui.add_space(22.0);
                ui.horizontal(|ui| {
                    if step > 0 && ui.button("Назад").clicked() {
                        self.experience.guide = Some(step - 1);
                    }
                    if ui
                        .add(colors.primary(if step + 1 == STEPS.len() {
                            "Начать работу"
                        } else {
                            "Далее"
                        }))
                        .clicked()
                    {
                        if step + 1 == STEPS.len() {
                            done = true;
                        } else {
                            self.experience.guide = Some(step + 1);
                        }
                    }
                    if ui.button("Пропустить").clicked() {
                        done = true;
                    }
                });
                if let Some(error) = &self.error {
                    ui.label(RichText::new(error).color(colors.orange).size(12.0));
                }
            });
        if done || response.should_close() {
            let previous = self.settings.experience.clone();
            self.settings.experience.guide_seen = true;
            if !self.experience.news {
                self.settings.experience.last_seen_version = env!("CARGO_PKG_VERSION").into();
            }
            if self.save_preferences() {
                self.experience.guide = None;
            } else {
                self.settings.experience = previous;
            }
        }
    }

    fn news_dialog(&mut self, ctx: &egui::Context) {
        let colors = theme::colors(ctx);
        let mut done = false;
        let response = egui::Modal::new(egui::Id::new("release-highlights"))
            .frame(colors.dialog_frame().inner_margin(24))
            .show(ctx, |ui| {
                ui.set_width(570.0);
                theme::heading(
                    ui,
                    "Что нового",
                    &format!(
                        "SoftDownloader {} · коротко о главном",
                        env!("CARGO_PKG_VERSION")
                    ),
                );
                egui::ScrollArea::vertical()
                    .max_height(330.0)
                    .show(ui, |ui| {
                        egui::Grid::new("highlights-table")
                            .num_columns(2)
                            .spacing(egui::vec2(18.0, 14.0))
                            .show(ui, |ui| {
                                for (area, description) in CHANGES {
                                    ui.label(RichText::new(*area).strong().color(colors.accent));
                                    ui.vertical(|ui| {
                                        ui.set_width(405.0);
                                        ui.label(*description);
                                    });
                                    ui.end_row();
                                }
                            });
                    });
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if ui.add(colors.primary("Понятно")).clicked() {
                        done = true;
                    }
                    ui.hyperlink_to("Полный список изменений", crate::updater::RELEASES_URL);
                });
                if let Some(error) = &self.error {
                    ui.label(RichText::new(error).color(colors.orange).size(12.0));
                }
            });
        if done || response.should_close() {
            let previous = self.settings.experience.last_seen_version.clone();
            self.settings.experience.last_seen_version = env!("CARGO_PKG_VERSION").into();
            if self.save_preferences() {
                self.experience.news = false;
            } else {
                self.settings.experience.last_seen_version = previous;
            }
        }
    }
}
