mod actions;
mod app;
mod appearance_view;
mod atmosphere;
mod background;
mod background_view;
mod catalog_controls;
mod catalog_icons;
mod catalog_search;
mod catalog_view;
#[cfg(feature = "catalog-tools")]
mod client_preview;
#[cfg(test)]
mod client_tests;
#[cfg(feature = "catalog-tools")]
pub use client_preview::run_client_preview;
mod confirmations;
mod experience;
mod icons;
#[cfg(test)]
mod installation_tests;
mod installation_view;
mod installed_search;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod performance;
mod program_lists;
mod queue_summary;
mod queue_view;
mod selection_view;
mod sets_view;
mod settings_view;
mod sky;
mod sound_view;
mod sounds;
pub mod theme;
mod update_view;
mod video_player;
#[cfg(feature = "catalog-tools")]
mod weather_preview;
mod weather_scene_view;
mod weather_transition;
mod weather_view;
#[cfg(feature = "catalog-tools")]
pub use weather_preview::run_weather_preview;

pub use app::SoftDownloaderApp;
