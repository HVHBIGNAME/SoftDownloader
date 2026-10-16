use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Lighting {
    #[default]
    Automatic,
    Dawn,
    Day,
    Evening,
    Dusk,
    Night,
}

impl Lighting {
    pub const ALL: [Self; 6] = [
        Self::Automatic,
        Self::Dawn,
        Self::Day,
        Self::Evening,
        Self::Dusk,
        Self::Night,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Авто",
            Self::Dawn => "Рассвет",
            Self::Day => "День",
            Self::Evening => "Вечер",
            Self::Dusk => "Закат",
            Self::Night => "Ночь",
        }
    }

    pub fn resolve(self, latitude: f64, longitude: f64, unix: u64) -> Sunlight {
        match self {
            Self::Automatic => Sunlight::at(latitude, longitude, unix),
            Self::Dawn => Sunlight::from_position(1.0, 85.0),
            Self::Day => Sunlight::from_position(42.0, 145.0),
            Self::Evening => Sunlight::from_position(14.0, 250.0),
            Self::Dusk => Sunlight::from_position(-1.0, 275.0),
            Self::Night => Sunlight::from_position(-35.0, 330.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sunlight {
    pub elevation: f32,
    pub azimuth: f32,
    pub daylight: f32,
    pub warmth: f32,
    pub phase: Lighting,
}

impl Sunlight {
    pub fn at(latitude: f64, longitude: f64, unix: u64) -> Self {
        // Low-precision solar coordinates (J2000); UTC and longitude, never the PC time zone.
        let days = unix as f64 / 86400.0 - 10957.5;
        let anomaly = (357.529 + 0.98560028 * days).to_radians();
        let ecliptic =
            (280.459 + 0.98564736 * days + 1.915 * anomaly.sin() + 0.020 * (2.0 * anomaly).sin())
                .to_radians();
        let obliquity = (23.439 - 0.00000036 * days).to_radians();
        let declination = (obliquity.sin() * ecliptic.sin()).asin();
        let ascension = (obliquity.cos() * ecliptic.sin()).atan2(ecliptic.cos());
        let hour = (280.16 + 360.9856235 * days + longitude).to_radians() - ascension;
        let lat = latitude.to_radians();
        let elevation = (lat.sin() * declination.sin()
            + lat.cos() * declination.cos() * hour.cos())
        .clamp(-1.0, 1.0)
        .asin()
        .to_degrees();
        let azimuth = (hour
            .sin()
            .atan2(hour.cos() * lat.sin() - declination.tan() * lat.cos())
            .to_degrees()
            + 180.0)
            .rem_euclid(360.0);
        Self::from_position(elevation as f32, azimuth as f32)
    }

    fn from_position(elevation: f32, azimuth: f32) -> Self {
        let x = ((elevation + 8.0) / 20.0).clamp(0.0, 1.0);
        Self {
            elevation,
            azimuth,
            daylight: x * x * (3.0 - 2.0 * x),
            warmth: smooth(-8.0, 2.0, elevation) * (1.0 - smooth(2.0, 24.0, elevation)),
            phase: if elevation < -8.0 {
                Lighting::Night
            } else if elevation < 8.0 {
                if azimuth < 180.0 {
                    Lighting::Dawn
                } else {
                    Lighting::Dusk
                }
            } else if elevation < 18.0 && azimuth >= 180.0 {
                Lighting::Evening
            } else {
                Lighting::Day
            },
        }
    }

    pub fn mix(self, target: Self, amount: f32) -> Self {
        let lerp = |a: f32, b: f32| a + (b - a) * amount;
        Self {
            elevation: lerp(self.elevation, target.elevation),
            azimuth: (self.azimuth
                + ((target.azimuth - self.azimuth + 180.0).rem_euclid(360.0) - 180.0) * amount)
                .rem_euclid(360.0),
            daylight: lerp(self.daylight, target.daylight),
            warmth: lerp(self.warmth, target.warmth),
            phase: target.phase,
        }
    }
}

fn smooth(a: f32, b: f32, value: f32) -> f32 {
    let t = ((value - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(iso: &str) -> u64 {
        time::OffsetDateTime::parse(iso, &time::format_description::well_known::Rfc3339)
            .unwrap()
            .unix_timestamp() as u64
    }

    #[test]
    fn solar_cycle_tracks_longitude_seasons_and_polar_day_without_a_timezone() {
        let noon = stamp("2026-03-20T12:00:00Z");
        assert_eq!(Sunlight::at(0.0, 0.0, noon).phase, Lighting::Day);
        assert_eq!(Sunlight::at(0.0, 180.0, noon).phase, Lighting::Night);
        assert_eq!(
            Sunlight::at(0.0, 0.0, noon - 6 * 3600).phase,
            Lighting::Dawn
        );
        assert_eq!(
            Sunlight::at(0.0, 0.0, noon + 6 * 3600).phase,
            Lighting::Dusk
        );
        let summer = stamp("2026-06-21T00:00:00Z");
        let winter = stamp("2026-12-21T12:00:00Z");
        assert!(Sunlight::at(78.0, 15.0, summer).elevation > 0.0);
        assert!(Sunlight::at(78.0, 15.0, winter).elevation < -8.0);
        assert!(Sunlight::at(-78.0, 15.0, winter).elevation > 0.0);
        for lat in [-90.0, 0.0, 90.0] {
            let sun = Sunlight::at(lat, 0.0, noon);
            assert!(sun.elevation.is_finite() && sun.azimuth.is_finite());
        }
    }

    #[test]
    fn sunrise_light_is_continuous_and_manual_modes_are_distinct() {
        let before = Sunlight::from_position(-0.84, 90.0);
        let after = Sunlight::from_position(-0.82, 90.0);
        assert!((before.daylight - after.daylight).abs() < 0.01);
        let day = Lighting::Day.resolve(0.0, 0.0, 0);
        let night = Lighting::Night.resolve(0.0, 0.0, 0);
        assert_eq!(day.daylight, 1.0);
        assert_eq!(night.daylight, 0.0);
        assert_eq!(day.mix(night, 0.5).daylight, 0.5);
    }

    #[test]
    fn a_real_day_traverses_dawn_day_evening_sunset_night_in_order() {
        let start = stamp("2026-03-20T00:00:00Z");
        let mut phases = Vec::new();
        for minute in 0..1440 {
            let phase = Sunlight::at(45.0, 0.0, start + minute * 60).phase;
            if phases.last() != Some(&phase) {
                phases.push(phase);
            }
        }
        assert_eq!(
            phases,
            [
                Lighting::Night,
                Lighting::Dawn,
                Lighting::Day,
                Lighting::Evening,
                Lighting::Dusk,
                Lighting::Night
            ]
        );
    }
}
