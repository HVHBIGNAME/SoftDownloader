use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{network, storage::Store};

pub const SEARCH_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";
pub const FORECAST_URL: &str = "https://api.met.no/weatherapi/locationforecast/2.0/compact";
pub const LOCATION_URL: &str = "https://ipwho.is/";
pub const REFRESH: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct City {
    pub name: String,
    #[serde(default)]
    pub country: String,
    #[serde(default, alias = "region")]
    pub admin1: String,
    pub latitude: f64,
    pub longitude: f64,
}

impl City {
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        for part in [&self.name, &self.admin1, &self.country] {
            if !part.is_empty() && !parts.contains(&part) {
                parts.push(part);
            }
        }
        parts
            .into_iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 300,
            "Некорректное название города"
        );
        ensure!(
            (-90.0..=90.0).contains(&self.latitude) && (-180.0..=180.0).contains(&self.longitude),
            "Некорректные координаты города"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherKind {
    Clear,
    Clouds,
    Rain,
    Snow,
    Blizzard,
    Fog,
    Storm,
}

impl WeatherKind {
    pub const ALL: [Self; 7] = [
        Self::Clear,
        Self::Clouds,
        Self::Rain,
        Self::Snow,
        Self::Blizzard,
        Self::Fog,
        Self::Storm,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Clear => "Ясно",
            Self::Clouds => "Облачно",
            Self::Rain => "Дождь",
            Self::Snow => "Снег",
            Self::Blizzard => "Метель",
            Self::Fog => "Туман",
            Self::Storm => "Гроза",
        }
    }
    pub fn film(self, day: bool) -> &'static str {
        match self {
            Self::Clear if day => "weather-clear",
            Self::Clear => "night-mountains",
            Self::Clouds => "weather-clouds",
            Self::Rain => "weather-rain",
            Self::Snow => "winter-forest",
            Self::Blizzard => "weather-blizzard",
            Self::Fog => "weather-fog",
            Self::Storm => "weather-storm",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Conditions {
    pub kind: WeatherKind,
    pub temperature: f32,
    pub wind_kmh: f32,
    pub precipitation: f32,
    pub day: bool,
}

impl Conditions {
    pub fn demo(kind: WeatherKind) -> Self {
        Self {
            kind,
            temperature: 10.0,
            wind_kmh: if kind == WeatherKind::Blizzard {
                45.0
            } else {
                10.0
            },
            precipitation: 1.0,
            day: true,
        }
    }
    fn validate(self) -> Result<()> {
        ensure!(
            (-120.0..=80.0).contains(&self.temperature)
                && (0.0..=500.0).contains(&self.wind_kmh)
                && (0.0..=500.0).contains(&self.precipitation),
            "Погодный сервис вернул некорректные значения"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Snapshot {
    pub city: City,
    pub conditions: Conditions,
    pub forecast_time: String,
    pub received_at: u64,
}

impl Snapshot {
    pub fn age(&self) -> Duration {
        Duration::from_secs(now().saturating_sub(self.received_at))
    }
    fn validate(&self) -> Result<()> {
        self.city.validate()?;
        self.conditions.validate()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct WeatherSettings {
    pub enabled: bool,
    pub city: Option<City>,
    pub cinematic: bool,
    pub dimming: u8,
}

impl Default for WeatherSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            city: None,
            cinematic: true,
            dimming: 58,
        }
    }
}

pub enum Request {
    Search(String),
    Locate,
    Forecast(City),
    Film(crate::backgrounds::VideoPreset),
}
impl Request {
    pub fn slot(&self) -> usize {
        match self {
            Self::Search(_) | Self::Locate => 0,
            Self::Forecast(_) => 1,
            Self::Film(_) => 2,
        }
    }
}
pub enum Response {
    Cities(Vec<City>),
    Forecast(Snapshot),
    Film { id: String, path: PathBuf },
}

pub async fn search(client: &reqwest::Client, query: &str) -> Result<Vec<City>> {
    ensure!(
        (2..=80).contains(&query.trim().chars().count()),
        "Введите от 2 до 80 символов"
    );
    let mut url = url::Url::parse(SEARCH_URL)?;
    url.query_pairs_mut().extend_pairs([
        ("name", query.trim()),
        ("count", "8"),
        ("language", "ru"),
        ("format", "json"),
    ]);
    #[derive(Deserialize)]
    struct Found {
        #[serde(default)]
        results: Vec<City>,
    }
    let found: Found = json(client, url.as_str()).await?;
    for city in &found.results {
        city.validate()?;
    }
    Ok(found.results)
}

pub async fn locate(client: &reqwest::Client) -> Result<Vec<City>> {
    #[derive(Deserialize)]
    struct Location {
        success: bool,
        city: Option<String>,
        country: Option<String>,
        region: Option<String>,
        latitude: Option<f64>,
        longitude: Option<f64>,
    }
    let found: Location = json(client, LOCATION_URL).await?;
    ensure!(found.success, "Город по IP определить не удалось");
    let city = City {
        name: found.city.context("Нет города в ответе")?,
        country: found.country.unwrap_or_default(),
        admin1: found.region.unwrap_or_default(),
        latitude: found.latitude.context("Нет широты")?,
        longitude: found.longitude.context("Нет долготы")?,
    };
    city.validate()?;
    Ok(vec![city])
}

pub async fn forecast(client: &reqwest::Client, city: City) -> Result<Snapshot> {
    city.validate()?;
    let mut url = url::Url::parse(FORECAST_URL)?;
    url.query_pairs_mut().extend_pairs([
        ("lat", format!("{:.4}", city.latitude)),
        ("lon", format!("{:.4}", city.longitude)),
    ]);
    let document: serde_json::Value = json(client, url.as_str()).await?;
    parse_forecast(&document, city, now())
}

fn parse_forecast(document: &serde_json::Value, city: City, timestamp: u64) -> Result<Snapshot> {
    let series = document
        .pointer("/properties/timeseries")
        .and_then(serde_json::Value::as_array)
        .context("Нет погодных данных")?;
    let (at, current) = series
        .iter()
        .filter_map(|entry| {
            let at = time::OffsetDateTime::parse(
                entry["time"].as_str()?,
                &time::format_description::well_known::Rfc3339,
            )
            .ok()?
            .unix_timestamp();
            (at >= 0 && at as u64 <= timestamp).then_some((at as u64, entry))
        })
        .max_by_key(|(at, _)| *at)
        .context("Нет прогноза на текущий момент")?;
    ensure!(
        timestamp.saturating_sub(at) <= 7200,
        "Погодный сервис вернул устаревший прогноз"
    );
    let details = &current["data"]["instant"]["details"];
    let period = current["data"]
        .get("next_1_hours")
        .or_else(|| current["data"].get("next_6_hours"))
        .context("Нет состояния погоды")?;
    let symbol = period["summary"]["symbol_code"]
        .as_str()
        .context("Нет кода погоды")?;
    let wind = details["wind_speed"]
        .as_f64()
        .context("Нет данных о ветре")? as f32
        * 3.6;
    let conditions = Conditions {
        kind: kind_from_symbol(symbol, wind)?,
        temperature: details["air_temperature"]
            .as_f64()
            .context("Нет температуры")? as f32,
        wind_kmh: wind,
        precipitation: period["details"]["precipitation_amount"]
            .as_f64()
            .unwrap_or(0.0) as f32,
        day: if symbol.ends_with("_night") {
            false
        } else if symbol.ends_with("_day") {
            true
        } else {
            daylight(city.latitude, city.longitude, timestamp)
        },
    };
    let snapshot = Snapshot {
        city,
        conditions,
        forecast_time: current["time"]
            .as_str()
            .context("Нет времени прогноза")?
            .into(),
        received_at: timestamp,
    };
    snapshot.validate()?;
    Ok(snapshot)
}

fn kind_from_symbol(symbol: &str, wind: f32) -> Result<WeatherKind> {
    Ok(if symbol.contains("thunder") {
        WeatherKind::Storm
    } else if symbol.contains("snow") || symbol.contains("sleet") {
        if wind >= 35.0 {
            WeatherKind::Blizzard
        } else {
            WeatherKind::Snow
        }
    } else if symbol.contains("rain") {
        WeatherKind::Rain
    } else if symbol == "fog" {
        WeatherKind::Fog
    } else if symbol.starts_with("clearsky") || symbol.starts_with("fair") {
        WeatherKind::Clear
    } else if symbol.contains("cloudy") {
        WeatherKind::Clouds
    } else {
        anyhow::bail!("Неизвестное состояние погоды: {symbol}")
    })
}

fn daylight(latitude: f64, longitude: f64, unix: u64) -> bool {
    let days = unix as f64 / 86400.0 - 10957.5;
    let anomaly = (357.529 + 0.98560028 * days).to_radians();
    let longitude_sun =
        (280.459 + 0.98564736 * days + 1.915 * anomaly.sin() + 0.020 * (2.0 * anomaly).sin())
            .to_radians();
    let obliquity = 23.439_f64.to_radians();
    let declination = (obliquity.sin() * longitude_sun.sin()).asin();
    let right_ascension = (obliquity.cos() * longitude_sun.sin()).atan2(longitude_sun.cos());
    let hour_angle = (280.16 + 360.9856235 * days + longitude).to_radians() - right_ascension;
    let lat = latitude.to_radians();
    lat.sin() * declination.sin() + lat.cos() * declination.cos() * hour_angle.cos() > -0.0145
}

async fn json<T: DeserializeOwned>(client: &reqwest::Client, url: &str) -> Result<T> {
    let response = client
        .get(url)
        .header(
            "User-Agent",
            concat!(
                "SoftDownloader/",
                env!("CARGO_PKG_VERSION"),
                " (github.com/HVHBIGNAME/SoftDownloader)"
            ),
        )
        .timeout(Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?;
    Ok(serde_json::from_slice(
        &network::read_limited(response, 512 * 1024).await?,
    )?)
}

pub fn cached(store: &Store) -> Result<Option<Snapshot>> {
    let path = store.root.join("weather.json");
    if !path.exists() {
        return Ok(None);
    }
    ensure!(
        std::fs::metadata(&path)?.len() <= 64 * 1024,
        "Некорректный размер кэша погоды"
    );
    let snapshot: Snapshot = serde_json::from_slice(&std::fs::read(path)?)?;
    snapshot.validate()?;
    Ok(Some(snapshot))
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weather_symbols_distinguish_snow_wind_fog_and_night() {
        for (symbol, wind, expected) in [
            ("clearsky_night", 0.0, WeatherKind::Clear),
            ("cloudy", 10.0, WeatherKind::Clouds),
            ("heavyrain", 2.0, WeatherKind::Rain),
            ("snow", 10.0, WeatherKind::Snow),
            ("snowshowers_day", 40.0, WeatherKind::Blizzard),
            ("fog", 0.0, WeatherKind::Fog),
            ("lightrainandthunder", 10.0, WeatherKind::Storm),
        ] {
            assert_eq!(kind_from_symbol(symbol, wind).unwrap(), expected);
        }
        assert!(kind_from_symbol("unknown", 1.0).is_err());
        assert!(!daylight(0.0, 0.0, 0));
        assert!(daylight(0.0, 0.0, 43200));
    }
    #[test]
    fn malformed_weather_is_never_presented_as_real_conditions() {
        let city = City {
            name: "Test".into(),
            country: String::new(),
            admin1: String::new(),
            latitude: 0.0,
            longitude: 0.0,
        };
        assert!(parse_forecast(&serde_json::json!({}), city.clone(), now()).is_err());
        assert!(
            City {
                latitude: f64::NAN,
                ..city
            }
            .validate()
            .is_err()
        );
        assert!(!WeatherSettings::default().enabled);
    }

    #[test]
    fn forecast_uses_the_latest_current_hour_and_rejects_stale_or_future_data() {
        let city = City {
            name: "City".into(),
            country: String::new(),
            admin1: String::new(),
            latitude: 0.0,
            longitude: 0.0,
        };
        let entry = |hour, temp| serde_json::json!({"time":format!("2026-10-12T{hour:02}:00:00Z"),"data":{"instant":{"details":{"air_temperature":temp,"wind_speed":11.0}},"next_1_hours":{"summary":{"symbol_code":"snow"},"details":{"precipitation_amount":1.0}}}});
        let document = serde_json::json!({"properties":{"timeseries":[entry(10, 1.0),entry(12, 2.0),entry(13, 3.0)]}});
        let at = time::OffsetDateTime::parse(
            "2026-10-12T12:30:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap()
        .unix_timestamp() as u64;
        let result = parse_forecast(&document, city.clone(), at).unwrap();
        assert_eq!(result.conditions.temperature, 2.0);
        assert_eq!(result.conditions.kind, WeatherKind::Blizzard);
        assert!(parse_forecast(&document, city.clone(), at + 8 * 3600).is_err());
        assert!(parse_forecast(&document, city, at - 6 * 3600).is_err());
    }
}
