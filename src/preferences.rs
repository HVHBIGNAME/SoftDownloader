use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct BackgroundSettings {
    pub video: Option<std::path::PathBuf>,
    pub dimming: u8,
    pub paused: bool,
    pub effects: bool,
}

impl Default for BackgroundSettings {
    fn default() -> Self {
        Self {
            video: None,
            dimming: 72,
            paused: false,
            effects: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: Theme,
    pub accent: [u8; 3],
    pub season: SeasonMode,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            accent: [134, 185, 232],
            season: SeasonMode::Automatic,
        }
    }
}

impl Appearance {
    pub fn current_season(self) -> Option<Season> {
        #[cfg(windows)]
        let month = {
            let mut time = windows_sys::Win32::Foundation::SYSTEMTIME::default();
            unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut time) };
            time.wMonth
        };
        #[cfg(not(windows))]
        let month = 0;
        self.season.resolve(month)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Graphite,
    Midnight,
}

impl Theme {
    pub const ALL: [Self; 4] = [Self::Dark, Self::Light, Self::Graphite, Self::Midnight];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Тёмная",
            Self::Light => "Светлая",
            Self::Graphite => "Серая",
            Self::Midnight => "Ночная",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Dark => "Мягкий угольный фон",
            Self::Light => "Светлая и воздушная",
            Self::Graphite => "Нейтральный графит",
            Self::Midnight => "Глубокий синий",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeasonMode {
    #[default]
    Automatic,
    Off,
    Winter,
    Halloween,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Season {
    Winter,
    Halloween,
}

impl SeasonMode {
    pub fn resolve(self, month: u16) -> Option<Season> {
        match self {
            Self::Off => None,
            Self::Winter => Some(Season::Winter),
            Self::Halloween => Some(Season::Halloween),
            Self::Automatic => match month {
                12 | 1 => Some(Season::Winter),
                10 => Some(Season::Halloween),
                _ => None,
            },
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => "По календарю",
            Self::Off => "Выключено",
            Self::Winter => "Зима",
            Self::Halloween => "Хэллоуин",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct SoundSettings {
    pub enabled: bool,
    pub style: SoundStyle,
    pub volume: u8,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            style: SoundStyle::Glass,
            volume: 18,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundStyle {
    #[default]
    Glass,
    Wood,
    Digital,
}

impl SoundStyle {
    pub const ALL: [Self; 3] = [Self::Glass, Self::Wood, Self::Digital];

    pub fn label(self) -> &'static str {
        match self {
            Self::Glass => "Мягкие",
            Self::Wood => "Деревянные",
            Self::Digital => "Приглушённые",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Glass => "Тихие округлые касания без звона",
            Self::Wood => "Короткие тёплые щелчки",
            Self::Digital => "Сухой мягкий отклик без резких высоких частот",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_covers_exactly_the_requested_months() {
        let expected = [
            Some(Season::Winter),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Season::Halloween),
            None,
            Some(Season::Winter),
        ];
        for (month, season) in (1..=12).zip(expected) {
            assert_eq!(SeasonMode::Automatic.resolve(month), season);
        }
        assert_eq!(SeasonMode::Automatic.resolve(0), None);
        assert_eq!(SeasonMode::Automatic.resolve(13), None);
    }

    #[test]
    fn disabled_and_manual_modes_do_not_follow_the_calendar() {
        for month in 0..=13 {
            assert_eq!(SeasonMode::Off.resolve(month), None);
            assert_eq!(SeasonMode::Winter.resolve(month), Some(Season::Winter));
            assert_eq!(
                SeasonMode::Halloween.resolve(month),
                Some(Season::Halloween)
            );
        }
    }
}
