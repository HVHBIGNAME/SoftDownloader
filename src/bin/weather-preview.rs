use anyhow::{Context, Result, ensure};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2 && args[0] == "--data-dir",
        "Usage: weather-preview --data-dir <isolated preview profile>"
    );
    let store = softdownloader::storage::Store::at(args[1].clone().into())?;
    softdownloader::ui::run_weather_preview(store)
        .map_err(|error| anyhow::anyhow!(error.to_string()))
        .context("Weather preview")
}
