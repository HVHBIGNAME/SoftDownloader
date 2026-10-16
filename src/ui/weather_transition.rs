use crate::weather::{Conditions, WeatherKind};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WeatherMix {
    weights: [f32; WeatherKind::ALL.len()],
    pub wind: f32,
}

impl WeatherMix {
    pub fn new(conditions: Conditions) -> Self {
        Self {
            weights: WeatherKind::ALL.map(|kind| if kind == conditions.kind { 1.0 } else { 0.0 }),
            wind: conditions.wind_kmh,
        }
    }

    pub fn weight(self, kind: WeatherKind) -> f32 {
        WeatherKind::ALL
            .iter()
            .zip(self.weights)
            .find(|(k, _)| **k == kind)
            .map_or(0.0, |(_, w)| w)
    }

    pub fn cover(self) -> f32 {
        WeatherKind::ALL
            .iter()
            .zip(self.weights)
            .map(|(kind, w)| {
                w * match kind {
                    WeatherKind::Clear => 0.12,
                    WeatherKind::Clouds => 0.65,
                    WeatherKind::Fog => 0.85,
                    WeatherKind::Storm
                    | WeatherKind::Blizzard
                    | WeatherKind::Downpour
                    | WeatherKind::Hail => 1.0,
                    _ => 0.75,
                }
            })
            .sum()
    }

    pub fn rain(self) -> f32 {
        self.weight(WeatherKind::Rain) * 0.65
            + self.weight(WeatherKind::Downpour)
            + self.weight(WeatherKind::Storm) * 0.9
    }

    pub fn snow(self) -> f32 {
        self.weight(WeatherKind::Snow) * 0.6 + self.weight(WeatherKind::Blizzard)
    }

    fn mix(self, other: Self, t: f32) -> Self {
        Self {
            weights: std::array::from_fn(|i| {
                self.weights[i] + (other.weights[i] - self.weights[i]) * t
            }),
            wind: self.wind + (other.wind - self.wind) * t,
        }
    }
}

pub(super) struct WeatherTransition {
    pub current: WeatherMix,
    from: WeatherMix,
    target: WeatherMix,
    elapsed: f32,
}

impl WeatherTransition {
    pub fn new(conditions: Conditions) -> Self {
        let current = WeatherMix::new(conditions);
        Self {
            current,
            from: current,
            target: current,
            elapsed: 2.4,
        }
    }

    pub fn update(&mut self, conditions: Conditions, dt: f32, immediate: bool) {
        let target = WeatherMix::new(conditions);
        if target != self.target {
            self.from = self.current;
            self.target = target;
            self.elapsed = 0.0;
        }
        self.elapsed = if immediate {
            2.4
        } else {
            (self.elapsed + dt).min(2.4)
        };
        let t = self.elapsed / 2.4;
        self.current = if self.elapsed >= 2.4 {
            self.target
        } else {
            self.from.mix(self.target, t * t * (3.0 - 2.0 * t))
        };
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Motion {
    pub time: f32,
    pub wind: f32,
    pub rain: f32,
    pub snow: f32,
    pub hail: f32,
    pub cloud: f32,
}

impl Motion {
    pub fn advance(&mut self, dt: f32, weather: WeatherMix) {
        self.time += dt;
        self.wind += dt * weather.wind.min(150.0);
        self.rain += dt * (650.0 + 260.0 * weather.rain());
        self.snow += dt * (80.0 + 120.0 * weather.weight(WeatherKind::Blizzard));
        self.hail += dt * 730.0;
        self.cloud += dt * (16.0 + weather.wind.min(100.0) * 0.45);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_transitions_start_at_current_weights_and_keep_a_fixed_budget() {
        let rain = Conditions::demo(WeatherKind::Rain);
        let snow = Conditions::demo(WeatherKind::Snow);
        let mut blend = WeatherTransition::new(rain);
        blend.update(snow, 0.0, false);
        assert_eq!(blend.current, WeatherMix::new(rain));
        blend.update(snow, 1.2, false);
        assert!((blend.current.weight(WeatherKind::Rain) - 0.5).abs() < 0.0001);
        let middle = blend.current;
        let hail = Conditions::demo(WeatherKind::Hail);
        blend.update(hail, 0.0, false);
        assert_eq!(blend.current, middle);
        for kind in WeatherKind::ALL.into_iter().cycle().take(180) {
            blend.update(Conditions::demo(kind), 0.016, false);
            assert!((blend.current.weights.iter().sum::<f32>() - 1.0).abs() < 0.0001);
            assert!(
                blend
                    .current
                    .weights
                    .iter()
                    .all(|w| (0.0..=1.0).contains(w))
            );
        }
        blend.update(hail, 3.0, false);
        assert_eq!(blend.current, WeatherMix::new(hail));
    }

    #[test]
    fn changing_wind_cannot_teleport_particles_after_long_uptime() {
        let mut motion = Motion::default();
        motion.advance(3600.0, WeatherMix::new(Conditions::demo(WeatherKind::Rain)));
        let before = motion;
        motion.advance(
            0.016,
            WeatherMix::new(Conditions::demo(WeatherKind::Blizzard)),
        );
        assert!(motion.wind - before.wind < 1.0);
        assert!(motion.snow - before.snow < 4.0);
        let frozen = motion.wind;
        motion.advance(0.0, WeatherMix::new(Conditions::demo(WeatherKind::Storm)));
        assert_eq!(motion.wind, frozen);
    }
}
