use std::collections::BTreeSet;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use futures_util::{StreamExt, stream};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::catalog::{Artifact, CatalogDocument, validate_https_url};
use crate::network::read_limited;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Github {
        repository: String,
        asset_pattern: String,
    },
    Website {
        page_url: String,
        link_selector: String,
        version_selector: String,
        download_hosts: Vec<String>,
    },
}

impl Source {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Github {
                repository,
                asset_pattern,
            } => {
                ensure!(
                    repository.split('/').count() == 2
                        && repository.split('/').all(|part| !part.is_empty()
                            && part != "."
                            && part != ".."
                            && part
                                .bytes()
                                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))),
                    "Репозиторий должен иметь вид owner/repository"
                );
                ensure!(
                    asset_pattern.len() <= 256,
                    "Слишком длинный шаблон имени файла"
                );
                Regex::new(asset_pattern).context("Некорректный шаблон GitHub-артефакта")?;
            }
            Self::Website {
                page_url,
                link_selector,
                version_selector,
                download_hosts,
            } => {
                validate_https_url(page_url)?;
                selector(link_selector)?;
                selector(version_selector)?;
                ensure!(
                    !download_hosts.is_empty() && download_hosts.len() <= 8,
                    "Укажите допустимые домены скачивания"
                );
                for host in download_hosts {
                    let url = validate_https_url(&format!("https://{host}/"))?;
                    ensure!(
                        url.host_str() == Some(host.as_str())
                            && url.path() == "/"
                            && url.port().is_none(),
                        "Неверный домен скачивания"
                    );
                }
            }
        }
        Ok(())
    }
}

pub async fn resolve(client: &Client, mut document: CatalogDocument) -> Result<CatalogDocument> {
    let jobs: Vec<_> = document
        .catalog
        .packages
        .iter()
        .enumerate()
        .filter(|(_, p)| p.enabled)
        .filter_map(|(index, package)| package.source.clone().map(|source| (index, source)))
        .collect();
    let results = stream::iter(jobs)
        .map(|(index, source)| async move {
            let result =
                tokio::time::timeout(Duration::from_secs(30), resolve_source(client, &source))
                    .await
                    .context("Источник не ответил за 30 секунд")
                    .and_then(|result| result);
            (index, result)
        })
        .buffer_unordered(4)
        .collect::<Vec<_>>()
        .await;
    for (index, result) in results {
        let package = &mut document.catalog.packages[index];
        match result {
            Ok((version, artifact)) => {
                package.version = version;
                package.artifact = Some(artifact);
            }
            Err(error) => {
                package.artifact = None;
                document
                    .diagnostics
                    .insert(package.id.clone(), format!("{error:#}"));
            }
        }
    }
    document.catalog.validate()?;
    Ok(document)
}

async fn resolve_source(client: &Client, source: &Source) -> Result<(String, Artifact)> {
    match source {
        Source::Github {
            repository,
            asset_pattern,
        } => {
            let url = format!("https://api.github.com/repos/{repository}/releases/latest");
            let response = client
                .get(url)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .await?
                .error_for_status()
                .context("GitHub не отдал релиз (возможен лимит API)")?;
            let bytes = read_limited(response, 4 * 1024 * 1024).await?;
            github_release(&bytes, repository, asset_pattern)
        }
        Source::Website {
            page_url,
            link_selector,
            version_selector,
            download_hosts,
        } => {
            let response = client
                .get(validate_https_url(page_url)?)
                .send()
                .await?
                .error_for_status()?;
            let final_url = response.url().clone();
            let bytes = read_limited(response, 4 * 1024 * 1024).await?;
            website_release(
                &String::from_utf8_lossy(&bytes),
                &final_url,
                link_selector,
                version_selector,
                download_hosts,
            )
        }
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

fn github_release(bytes: &[u8], repository: &str, pattern: &str) -> Result<(String, Artifact)> {
    let release: Release = serde_json::from_slice(bytes).context("Некорректный ответ GitHub")?;
    let pattern = Regex::new(pattern)?;
    let matches: Vec<_> = release
        .assets
        .into_iter()
        .filter(|asset| pattern.is_match(&asset.name))
        .collect();
    ensure!(
        matches.len() == 1,
        "Найдено {} подходящих установщиков; нужен ровно один",
        matches.len()
    );
    let asset = matches.into_iter().next().context("Нет установщика")?;
    let url = validate_https_url(&asset.browser_download_url)?;
    ensure!(
        url.host_str() == Some("github.com")
            && url.path().to_ascii_lowercase().starts_with(&format!(
                "/{}/releases/download/",
                repository.to_ascii_lowercase()
            )),
        "Неожиданный адрес GitHub-артефакта"
    );
    let hash = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .context(
            "GitHub не опубликовал SHA-256 этого файла; нужен закреплённый artifact в каталоге",
        )?;
    ensure!(
        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "Некорректный SHA-256 от GitHub"
    );
    Ok((
        release.tag_name.trim_start_matches('v').to_owned(),
        Artifact {
            file_name: asset.name,
            size: asset.size,
            sha256: hash.to_ascii_lowercase(),
            url: Some(asset.browser_download_url),
            local_path: None,
            drive_file_id: None,
        },
    ))
}

fn selector(value: &str) -> Result<Selector> {
    Selector::parse(value).map_err(|error| anyhow::anyhow!("Некорректный CSS-селектор: {error}"))
}

fn website_release(
    html: &str,
    base: &Url,
    link_selector: &str,
    version_selector: &str,
    hosts: &[String],
) -> Result<(String, Artifact)> {
    let page = Html::parse_document(html);
    let links: BTreeSet<_> = page
        .select(&selector(link_selector)?)
        .filter_map(|element| element.value().attr("href"))
        .map(|link| base.join(link).map(|url| url.to_string()))
        .collect::<Result<_, _>>()?;
    ensure!(
        links.len() == 1,
        "На сайте не найдена однозначная ссылка установщика; обновите CSS-селектор"
    );
    let link = links.into_iter().next().context("Нет ссылки установщика")?;
    let url = validate_https_url(&link)?;
    ensure!(
        hosts
            .iter()
            .any(|host| Some(host.as_str()) == url.host_str()),
        "Сайт ссылается на неразрешённый домен загрузки"
    );
    let version_selector = selector(version_selector)?;
    let element = page
        .select(&version_selector)
        .next()
        .context("Версия на сайте не найдена")?;
    let version = element
        .value()
        .attr("content")
        .map(str::to_owned)
        .unwrap_or_else(|| element.text().collect::<String>())
        .trim()
        .to_owned();
    ensure!(
        !version.is_empty() && version.len() <= 80,
        "Некорректная версия на сайте"
    );
    let file_name = url
        .path_segments()
        .and_then(|mut parts| parts.next_back())
        .context("Нет имени файла в URL")?
        .to_owned();
    Ok((
        version,
        Artifact {
            file_name,
            size: 0,
            sha256: String::new(),
            url: Some(link),
            local_path: None,
            drive_file_id: None,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_selects_one_asset_and_requires_a_published_hash() {
        let value = serde_json::json!({"tag_name":"v1.2.3","assets":[{"name":"app-x64.exe","size":123,"digest":format!("sha256:{}", "a".repeat(64)),"browser_download_url":"https://github.com/vendor/app/releases/download/v1.2.3/app-x64.exe"}]});
        let (version, artifact) = github_release(
            &serde_json::to_vec(&value).unwrap(),
            "vendor/app",
            "x64\\.exe$",
        )
        .unwrap();
        assert_eq!(version, "1.2.3");
        assert_eq!(artifact.sha256, "a".repeat(64));
        assert!(
            github_release(&serde_json::to_vec(&value).unwrap(), "vendor/app", "arm64").is_err()
        );
        let mut missing = value;
        missing["assets"][0]["digest"] = serde_json::Value::Null;
        assert!(
            github_release(&serde_json::to_vec(&missing).unwrap(), "vendor/app", "x64").is_err()
        );
    }

    #[test]
    fn website_parses_metadata_and_rejects_unexpected_hosts() {
        let html = r#"<span itemprop="softwareVersion">10.10</span><a class="download" href="/downloads/setup.msi">Download</a>"#;
        let base = Url::parse("https://vendor.example/download").unwrap();
        let (version, artifact) = website_release(
            html,
            &base,
            "a.download",
            "[itemprop=softwareVersion]",
            &["vendor.example".into()],
        )
        .unwrap();
        assert_eq!(version, "10.10");
        assert_eq!(
            artifact.url.unwrap(),
            "https://vendor.example/downloads/setup.msi"
        );
        assert!(artifact.sha256.is_empty());
        assert!(
            website_release(
                &html.replace("/downloads/setup.msi", "https://other.example/setup.msi"),
                &base,
                "a.download",
                "[itemprop=softwareVersion]",
                &["vendor.example".into()]
            )
            .is_err()
        );
    }
}
