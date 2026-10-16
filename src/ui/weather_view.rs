use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

use super::{app::SoftDownloaderApp, theme};
use crate::weather::{
    self, City, Conditions, Lighting, Request, Response, Scene, Snapshot, Sunlight, WeatherSettings,
};

pub(super) struct WeatherUi {
    query: String,
    results: Vec<City>,
    search_at: Option<Instant>,
    pub snapshot: Option<Snapshot>,
    pub preview: Option<Conditions>,
    pub reveal_scene: bool,
    tickets: [u64; 3],
    busy: [bool; 3],
    next_refresh: Instant,
    film_retry: Instant,
    film_id: Option<String>,
    ready_film: Option<String>,
    video: Option<PathBuf>,
    errors: [Option<String>; 3],
    enabled: bool,
}

impl WeatherUi {
    pub fn new(store: &crate::storage::Store) -> Self {
        let (snapshot, error) = match weather::cached(store) {
            Ok(snapshot) => (snapshot, None),
            Err(e) => (None, Some(format!("Кэш погоды: {e:#}"))),
        };
        Self {
            query: String::new(),
            results: Vec::new(),
            search_at: None,
            snapshot,
            preview: None,
            reveal_scene: false,
            tickets: [0; 3],
            busy: [false; 3],
            next_refresh: Instant::now(),
            film_retry: Instant::now(),
            film_id: None,
            ready_film: None,
            video: None,
            errors: [None, error, None],
            enabled: false,
        }
    }

    pub fn scene(&self, settings: &WeatherSettings) -> Option<Conditions> {
        self.preview.or_else(|| {
            self.snapshot
                .as_ref()
                .filter(|s| {
                    settings.enabled
                        && settings.city.as_ref() == Some(&s.city)
                        && s.age() < Duration::from_secs(6 * 3600)
                })
                .map(|s| s.conditions)
        })
    }

    pub fn sunlight(&self, settings: &WeatherSettings, timestamp: u64) -> Sunlight {
        match &settings.city {
            Some(city) => settings
                .lighting
                .resolve(city.latitude, city.longitude, timestamp),
            None => match settings.lighting {
                Lighting::Automatic => Lighting::Day.resolve(0.0, 0.0, timestamp),
                mode => mode.resolve(0.0, 0.0, timestamp),
            },
        }
    }

    pub fn visual(&self, settings: &WeatherSettings) -> Option<Scene> {
        self.scene(settings).map(|mut conditions| {
            let sunlight = self.sunlight(settings, weather::now());
            conditions.day = sunlight.elevation > -0.833;
            Scene {
                conditions,
                sunlight,
            }
        })
    }
}

impl SoftDownloaderApp {
    fn weather_request(&mut self, request: Request) {
        let slot = request.slot();
        self.weather.tickets[slot] = self.weather.tickets[slot].wrapping_add(1);
        self.weather.busy[slot] = true;
        self.weather.errors[slot] = None;
        self.engine
            .weather(request, self.weather.tickets[slot], self.store.clone());
    }

    pub(super) fn receive_weather(
        &mut self,
        slot: usize,
        ticket: u64,
        result: Result<Response, String>,
    ) {
        if self.weather.tickets.get(slot) != Some(&ticket) {
            return;
        }
        self.weather.busy[slot] = false;
        match result {
            Ok(Response::Cities(cities)) => self.weather.results = cities,
            Ok(Response::Forecast(snapshot)) => {
                if self.settings.weather.city.as_ref() != Some(&snapshot.city) {
                    return;
                }
                if let Err(e) =
                    crate::storage::write_json(&self.store.root.join("weather.json"), &snapshot)
                {
                    self.weather.errors[1] = Some(format!("Не удалось сохранить кэш: {e:#}"));
                }
                self.weather.reveal_scene |= self.weather.snapshot.is_none();
                self.weather.snapshot = Some(snapshot);
                self.weather.next_refresh = Instant::now() + weather::REFRESH;
            }
            Ok(Response::Film { id, path }) => {
                if self.weather.film_id.as_deref() == Some(&id) {
                    self.weather.ready_film = Some(id);
                    self.weather.video = Some(path);
                }
            }
            Err(error) => {
                self.weather.errors[slot] = Some(error);
                if slot == 1 {
                    self.weather.next_refresh = Instant::now() + Duration::from_secs(120);
                }
                if slot == 2 {
                    self.weather.film_retry = Instant::now() + Duration::from_secs(300);
                }
            }
        }
    }

    pub(super) fn tick_weather(&mut self, ctx: &egui::Context) {
        let enabled = self.settings.weather.enabled || self.weather.preview.is_some();
        if !enabled && self.weather.enabled {
            self.engine.cancel_weather();
            self.weather.tickets = self.weather.tickets.map(|t| t.wrapping_add(1));
            self.weather.busy = [false; 3];
            self.weather.film_id = None;
            self.weather.ready_film = None;
            self.weather.video = None;
        }
        self.weather.enabled = enabled;
        if self
            .weather
            .search_at
            .is_some_and(|at| Instant::now() >= at)
        {
            self.weather.search_at = None;
            self.weather_request(Request::Search(self.weather.query.clone()));
        }
        if self.settings.weather.enabled
            && !self.weather.busy[1]
            && Instant::now() >= self.weather.next_refresh
            && let Some(city) = self.settings.weather.city.clone()
        {
            self.weather_request(Request::Forecast(city));
        }
        if enabled
            && self.settings.weather.cinematic
            && let Some(scene) = self.weather.visual(&self.settings.weather)
        {
            let id = scene.conditions.kind.film(scene.conditions.day);
            if (self.weather.film_id.as_deref() != Some(id)
                || (!self.weather.busy[2]
                    && self.weather.ready_film.as_deref() != Some(id)
                    && Instant::now() >= self.weather.film_retry))
                && let Some(preset) = self.background_presets.iter().find(|p| p.id == id).cloned()
            {
                self.weather.film_id = Some(id.into());
                self.weather_request(Request::Film(preset));
            }
        }
        if enabled {
            ctx.request_repaint_after(Duration::from_secs(15));
        }
        if self.weather.search_at.is_some() || self.weather.busy.iter().any(|b| *b) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    pub(super) fn weather_background(
        &self,
    ) -> (crate::preferences::BackgroundSettings, Option<Scene>) {
        let mut settings = self.settings.background.clone();
        let scene = self.weather.visual(&self.settings.weather);
        if scene.is_some() {
            settings.video = if self.settings.weather.cinematic {
                self.weather.video.clone()
            } else {
                None
            };
            settings.dimming = self.settings.weather.dimming;
        }
        (settings, scene)
    }

    fn choose_weather_city(&mut self, city: City) {
        let previous = self.settings.weather.clone();
        self.settings.weather.city = Some(city);
        self.settings.weather.enabled = true;
        if !self.save_preferences() {
            self.settings.weather = previous;
            return;
        }
        self.engine.cancel_weather();
        self.weather.tickets = self.weather.tickets.map(|t| t.wrapping_add(1));
        self.weather.busy = [false; 3];
        self.weather.preview = None;
        self.weather.snapshot = None;
        self.weather.results.clear();
        self.weather.search_at = None;
        self.weather.next_refresh = Instant::now();
        self.weather.query.clear();
    }

    pub(super) fn weather_page(&mut self, ui: &mut egui::Ui) {
        theme::heading(
            ui,
            "Погода",
            "Ваш город, живое небо и атмосферные сцены на фоне приложения.",
        );
        let previous = self.settings.weather.clone();
        let colors = theme::colors(ui.ctx());
        let mut scroll = egui::ScrollArea::vertical().id_salt("weather-settings");
        if std::mem::take(&mut self.weather.reveal_scene) {
            scroll = scroll.vertical_scroll_offset(0.0);
        }
        scroll.show(ui, |ui| {
            self.weather_showcase(ui);
            colors.card_frame().show(ui, |ui| {
                ui.set_width((ui.available_width() - 2.0).max(150.0));
                if ui.checkbox(&mut self.settings.weather.enabled, "Включить тему «Погода»").changed() {
                    self.weather.preview = None;
                    self.weather.next_refresh = Instant::now();
                }
                ui.label(RichText::new("Обновление каждые 15 минут. Город выбираете вы; определение по IP запускается только кнопкой ниже.").size(12.0).color(colors.muted));
                self.weather_city_search(ui);
            });
            ui.add_space(12.0);
            self.weather_conditions_card(ui);
            ui.add_space(12.0);
            self.weather_atmosphere(ui);
        });
        if previous != self.settings.weather && !self.save_preferences() {
            self.settings.weather = previous;
        }
    }

    fn weather_atmosphere(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        colors.card_frame().show(ui, |ui| {
            ui.set_width((ui.available_width() - 2.0).max(150.0));
            ui.label(RichText::new("Атмосфера").size(18.0).strong());
            if self.weather.scene(&self.settings.weather).is_none() {
                self.weather_lighting_controls(ui);
                self.weather_motion_controls(ui);
            }
            if ui.checkbox(&mut self.settings.weather.cinematic, "Добавить фактуру из видеосцен").changed() && !self.settings.weather.cinematic {
                self.engine.cancel_weather_film();
                self.weather.tickets[2] = self.weather.tickets[2].wrapping_add(1);
                self.weather.busy[2] = false;
                self.weather.film_id = None;
                self.weather.ready_film = None;
                self.weather.video = None;
            }
            ui.label(RichText::new("Небо, облака и осадки анимируются без видео. Дополнительная видеосцена кэшируется (до 10 МБ). Это иллюстрация погоды, не камера города.").size(12.0).color(colors.muted));
            theme::percent_slider(ui, &mut self.settings.weather.dimming, 45..=95, "Приглушение");
            if self.weather.scene(&self.settings.weather).is_none() {
                self.weather_demo_controls(ui);
            }
            if self.weather.busy[2] { ui.horizontal(|ui| { ui.spinner(); ui.label("Загружаем погодную сцену…"); }); }
            if let Some(error) = &self.weather.errors[2] { ui.label(RichText::new(error).color(colors.orange)); }
            if let Some(error) = &self.background.video.error { ui.label(RichText::new(error).color(colors.orange)); }
            ui.label(RichText::new("Видимое окно продолжает анимацию даже без фокуса. Сворачивание, пауза и уменьшение движения останавливают фон.").size(12.0).color(colors.muted));
            ui.horizontal_wrapped(|ui| { ui.hyperlink_to("Погода: MET Norway · CC BY 4.0", "https://api.met.no/"); ui.hyperlink_to("Города: Open-Meteo / GeoNames", "https://open-meteo.com/en/docs/geocoding-api"); ui.hyperlink_to("Видео: Mixkit", "https://mixkit.co/license/#videoFree"); });
        });
    }

    fn weather_city_search(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let input = ui.add(
                egui::TextEdit::singleline(&mut self.weather.query)
                    .hint_text("Начните вводить название города…")
                    .desired_width(300.0)
                    .char_limit(80),
            );
            if input.changed() {
                self.weather.results.clear();
                self.weather.errors[0] = None;
                self.weather.tickets[0] = self.weather.tickets[0].wrapping_add(1);
                self.weather.busy[0] = false;
                self.weather.search_at = (self.weather.query.trim().chars().count() >= 2)
                    .then(|| Instant::now() + Duration::from_millis(450));
            }
            if ui
                .add_enabled(!self.weather.busy[0], egui::Button::new("Определить по IP"))
                .clicked()
            {
                self.weather.search_at = None;
                self.weather.results.clear();
                self.weather_request(Request::Locate);
            }
            if self.weather.busy[0] {
                ui.spinner();
            }
        });
        ui.label(RichText::new("IP даёт приблизительный город и может указывать на VPN. Проверьте и выберите результат.").size(11.0).color(theme::colors(ui.ctx()).muted));
        if !self.weather.results.is_empty() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                for city in self.weather.results.clone() {
                    if ui.selectable_label(false, city.label()).clicked() {
                        self.choose_weather_city(city);
                    }
                }
            });
        } else if self.weather.query.trim().chars().count() == 1 {
            ui.label("Введите ещё хотя бы один символ.");
        } else if !self.weather.query.trim().is_empty()
            && !self.weather.busy[0]
            && self.weather.search_at.is_none()
            && self.weather.errors[0].is_none()
        {
            ui.label("Совпадений нет. Попробуйте другое написание города.");
        }
        if let Some(error) = &self.weather.errors[0] {
            ui.label(RichText::new(error).color(theme::colors(ui.ctx()).orange));
        }
    }

    fn weather_conditions_card(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        colors.card_frame().show(ui, |ui| {
            ui.set_width((ui.available_width() - 2.0).max(150.0));
            if let Some(city) = &self.settings.weather.city {
                ui.label(RichText::new(city.label()).size(20.0).strong());
            } else {
                ui.label("Выберите город выше или посмотрите демонстрацию эффектов.");
            }
            if let Some(snapshot) = self
                .weather
                .snapshot
                .as_ref()
                .filter(|s| self.settings.weather.city.as_ref() == Some(&s.city))
            {
                let c = snapshot.conditions;
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("{:.0}°", c.temperature))
                            .size(44.0)
                            .strong(),
                    );
                    ui.vertical(|ui| {
                        ui.label(format!(
                            "{} · {}",
                            c.kind.label(),
                            weather::Sunlight::at(
                                snapshot.city.latitude,
                                snapshot.city.longitude,
                                weather::now()
                            )
                            .phase
                            .label()
                        ));
                        ui.label(format!(
                            "Ветер {:.0} км/ч · осадки {:.1} мм/ч",
                            c.wind_kmh, c.precipitation
                        ));
                    });
                });
                let stale = snapshot.age() > weather::REFRESH * 2;
                ui.label(
                    RichText::new(format!(
                        "{} · получено {} мин. назад · прогноз {}",
                        if stale {
                            "Устаревшие данные"
                        } else {
                            "MET Norway"
                        },
                        snapshot.age().as_secs() / 60,
                        snapshot.forecast_time
                    ))
                    .size(11.0)
                    .color(if stale { colors.orange } else { colors.muted }),
                );
            }
            ui.horizontal_wrapped(|ui| {
                if self.weather.busy[1] {
                    ui.spinner();
                    ui.label("Обновляем погоду…");
                }
                if ui
                    .add_enabled(
                        self.settings.weather.enabled
                            && self.settings.weather.city.is_some()
                            && !self.weather.busy[1],
                        egui::Button::new("Обновить погоду"),
                    )
                    .clicked()
                {
                    self.weather.next_refresh = Instant::now();
                }
            });
            if let Some(error) = &self.weather.errors[1] {
                ui.label(
                    RichText::new(format!("Нет свежих данных: {error}"))
                        .color(colors.orange)
                        .size(12.0),
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather::WeatherKind;

    #[test]
    fn stale_requests_wrong_cities_and_disabled_weather_cannot_replace_the_scene() {
        let (_root, ctx, mut app) = super::super::installation_tests::fixture(false);
        let city = City {
            name: "City A".into(),
            country: String::new(),
            admin1: String::new(),
            latitude: 10.0,
            longitude: 20.0,
        };
        app.settings.weather.city = Some(city.clone());
        app.settings.weather.enabled = true;
        let snapshot = Snapshot {
            city: city.clone(),
            conditions: Conditions::demo(WeatherKind::Rain),
            forecast_time: "2026-10-12T00:00:00Z".into(),
            received_at: weather::now(),
        };
        app.receive_weather(1, 0, Ok(Response::Forecast(snapshot.clone())));
        assert_eq!(
            app.weather.scene(&app.settings.weather).unwrap().kind,
            WeatherKind::Rain
        );
        assert_eq!(weather::cached(&app.store).unwrap().unwrap().city, city);
        app.settings.weather.city = Some(City {
            name: "City B".into(),
            ..city.clone()
        });
        app.receive_weather(1, 0, Ok(Response::Forecast(snapshot.clone())));
        assert!(app.weather.scene(&app.settings.weather).is_none());
        app.settings.weather.city = Some(city);
        app.weather.snapshot.as_mut().unwrap().received_at = weather::now() - 7 * 3600;
        assert!(app.weather.scene(&app.settings.weather).is_none());
        app.weather.enabled = true;
        app.settings.weather.enabled = false;
        app.tick_weather(&ctx);
        app.receive_weather(1, 0, Ok(Response::Forecast(snapshot)));
        assert!(app.weather.scene(&app.settings.weather).is_none());
        app.receive_weather(0, 999, Ok(Response::Cities(vec![])));
        app.weather.preview = Some(Conditions::demo(WeatherKind::Fog));
        assert_eq!(
            app.weather.scene(&app.settings.weather).unwrap().kind,
            WeatherKind::Fog
        );
    }
}
