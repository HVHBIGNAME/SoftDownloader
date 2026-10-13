use anyhow::{Context, Result, ensure};

#[tokio::main]
async fn main() -> Result<()> {
    let query = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    ensure!(!query.is_empty(), "Usage: weather-check <city name>");
    let client = softdownloader::network::client()?;
    let cities = softdownloader::weather::search(&client, &query).await?;
    println!("{}", serde_json::to_string_pretty(&cities)?);
    let city = cities.into_iter().next().context("No matching city")?;
    let snapshot = softdownloader::weather::forecast(&client, city).await?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}
