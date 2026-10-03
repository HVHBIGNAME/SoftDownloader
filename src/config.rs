use anyhow::Result;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Channel {
    additional_catalog_url: Option<String>,
}

pub fn additional_catalog_url() -> Result<String> {
    let channel: Channel = serde_json::from_str(include_str!("../catalog/channel.json"))?;
    let source = option_env!("SOFTDOWNLOADER_CATALOG_URL")
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .or(channel.additional_catalog_url)
        .unwrap_or_default();
    if !source.trim().is_empty() {
        crate::catalog::validate_https_url(source.trim())?;
    }
    Ok(source.trim().to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_channel_is_valid_without_requiring_a_drive() {
        super::additional_catalog_url().unwrap();
    }
}
