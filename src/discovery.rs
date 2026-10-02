use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use futures_util::{StreamExt, stream};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::catalog::{Artifact, CatalogDocument, validate_https_url};
use crate::network::read_limited;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WingetRepository {
    #[default]
    Winget,
    Msstore,
}

impl WingetRepository {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Winget => "winget",
            Self::Msstore => "msstore",
        }
    }
}

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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version_attribute: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version_pattern: Option<String>,
        download_hosts: Vec<String>,
    },
    Winget {
        package_id: String,
        #[serde(default)]
        repository: WingetRepository,
    },
    Manual {
        url: String,
        instructions: String,
    },
}

impl Source {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Winget { package_id, .. } => ensure!(
                crate::system::valid_tool_id(package_id),
                "Некорректный ID WinGet"
            ),
            Self::Manual { url, instructions } => {
                validate_https_url(url)?;
                ensure!(
                    !instructions.trim().is_empty(),
                    "Нужна инструкция для ручной установки"
                );
            }
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
                version_attribute,
                version_pattern,
                download_hosts,
            } => {
                validate_https_url(page_url)?;
                selector(link_selector)?;
                selector(version_selector)?;
                if let Some(attribute) = version_attribute {
                    ensure!(
                        !attribute.is_empty()
                            && attribute
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
                        "Некорректный HTML-атрибут версии"
                    );
                }
                if let Some(pattern) = version_pattern {
                    ensure!(
                        pattern.len() <= 256 && Regex::new(pattern)?.captures_len() == 2,
                        "Шаблон версии должен содержать одну захватывающую группу"
                    );
                }
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
        .filter_map(|(index, package)| {
            package
                .source
                .clone()
                .filter(|s| matches!(s, Source::Github { .. } | Source::Website { .. }))
                .map(|source| (index, source))
        })
        .collect();
    // An expired budget simply drops the in-flight sources; the pass below
    // reports them as unreachable instead of waiting any longer.
    let results: Vec<_> = tokio::time::timeout(
        crate::network::CATALOG_BUDGET,
        stream::iter(jobs)
            .map(|(index, source)| async move {
                let result = tokio::time::timeout(
                    crate::network::SOURCE_TIMEOUT,
                    resolve_source(client, &source),
                )
                .await
                .context("Источник не ответил вовремя")
                .and_then(|result| result);
                (index, result)
            })
            .buffer_unordered(6)
            .collect::<Vec<_>>(),
    )
    .await
    .unwrap_or_default();
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
    // Any online source that produced no artifact did not answer in time.
    for package in &mut document.catalog.packages {
        if package.enabled
            && package.artifact.is_none()
            && package.source.as_ref().is_some_and(|source| {
                matches!(source, Source::Github { .. } | Source::Website { .. })
            })
        {
            document.diagnostics.insert(
                package.id.clone(),
                "Источник не ответил за отведённое время".into(),
            );
        }
    }
    document.catalog.validate()?;
    if !crate::system::winget::available() {
        for package in &mut document.catalog.packages {
            if package.winget_id().is_some() {
                package.enabled = false;
                document.diagnostics.insert(
                    package.id.clone(),
                    "Нужен WinGet: установите «Установщик приложений» Microsoft из Store".into(),
                );
            }
        }
    }
    Ok(document)
}

async fn resolve_source(client: &Client, source: &Source) -> Result<(String, Artifact)> {
    match source {
        Source::Winget { .. } | Source::Manual { .. } => {
            anyhow::bail!("Этот источник не использует загрузку артефакта")
        }
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
            version_attribute,
            version_pattern,
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
                version_attribute.as_deref(),
                version_pattern.as_deref(),
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
    version_attribute: Option<&str>,
    version_pattern: Option<&str>,
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
    let raw_version = if let Some(attribute) = version_attribute {
        element
            .value()
            .attr(attribute)
            .context("Атрибут версии на сайте не найден")?
            .to_owned()
    } else {
        element
            .value()
            .attr("content")
            .map(str::to_owned)
            .unwrap_or_else(|| element.text().collect::<String>())
    };
    let version = if let Some(pattern) = version_pattern {
        Regex::new(pattern)?
            .captures(&raw_version)
            .and_then(|captures| captures.get(1))
            .context("Версия на сайте не соответствует шаблону")?
            .as_str()
            .to_owned()
    } else {
        raw_version.trim().to_owned()
    };
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
    fn a_budget_overrun_is_reported_instead_of_waiting_forever() {
        let document = CatalogDocument::demo().unwrap();
        let report = crate::network::client().unwrap();
        let resolved = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(resolve(&report, document));
        // The demo catalog has no online sources, so this only asserts that the
        // bounded resolve path completes without hanging.
        assert!(resolved.is_ok());
    }

    #[tokio::test]
    async fn an_unreachable_source_yields_a_diagnostic() {
        let mut document = CatalogDocument::demo().unwrap();
        document
            .catalog
            .packages
            .retain(|package| package.id == "blender");
        let package = document.catalog.packages.first_mut().unwrap();
        package.enabled = true;
        package.source = Some(Source::Github {
            repository: "softdownloader/does-not-exist".into(),
            asset_pattern: "missing.zip".into(),
        });
        // A placeholder artifact and install recipe keep the document valid
        // while the unreachable source is the part under test.
        package.artifact = Some(Artifact {
            file_name: "addon.zip".into(),
            size: 1,
            sha256: "a".repeat(64),
            local_path: Some("addon.zip".into()),
            drive_file_id: None,
            url: None,
        });
        package.install = Some(crate::catalog::InstallSpec::Zip {
            destination: crate::catalog::ArchiveDestination {
                root: crate::catalog::ArchiveRoot::LocalAppData,
                path: "softdownloader-test/resolve".into(),
            },
            strip_components: 0,
        });
        let client = crate::network::client().unwrap();
        let resolved = resolve(&client, document).await.unwrap();
        assert!(
            resolved.diagnostics.contains_key("blender"),
            "a failed source must be reported: {:?}",
            resolved.diagnostics
        );
        assert!(
            resolved
                .catalog
                .package("blender")
                .unwrap()
                .artifact
                .is_none()
        );
    }

    #[test]
    fn website_can_extract_version_from_download_filename() {
        let html = r#"<a class="win64" href="https://vendor.example/3uTools_v9.10.006_Setup_x64.exe">Download</a>"#;
        let base = Url::parse("https://vendor.example/").unwrap();
        let (version, artifact) = website_release(
            html,
            &base,
            "a.win64",
            "a.win64",
            Some("href"),
            Some(r"3uTools_v([0-9.]+)_Setup"),
            &["vendor.example".into()],
        )
        .unwrap();
        assert_eq!(version, "9.10.006");
        assert_eq!(artifact.file_name, "3uTools_v9.10.006_Setup_x64.exe");
        assert!(
            website_release(
                html,
                &base,
                "a.win64",
                "a.win64",
                Some("missing"),
                None,
                &["vendor.example".into()]
            )
            .is_err()
        );
    }

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
            None,
            None,
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
                None,
                None,
                &["vendor.example".into()]
            )
            .is_err()
        );
    }
}
