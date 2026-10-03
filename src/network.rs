use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use futures_util::StreamExt;
use reqwest::{Client, Response, header, redirect};
use scraper::{Html, Selector};
use url::Url;

use crate::catalog::{Catalog, CatalogDocument, validate_drive_id, validate_https_url};

pub const MAX_CATALOG_BYTES: usize = 8 * 1024 * 1024;
const MAX_CONFIRMATION_BYTES: usize = 512 * 1024;

/// Total time budget for resolving every online source of the catalog.
///
/// The catalog has a handful of GitHub and website sources; without a global
/// budget a slow host would keep the window waiting on a long chain of retries.
pub const CATALOG_BUDGET: Duration = Duration::from_secs(20);
pub const SOURCE_TIMEOUT: Duration = Duration::from_secs(8);

pub fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(concat!("SoftDownloader/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(5))
        .read_timeout(Duration::from_secs(30))
        .cookie_store(true)
        .https_only(true)
        .redirect(redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 8 {
                attempt.error("Слишком много перенаправлений")
            } else if attempt.url().scheme() != "https"
                || !attempt.url().username().is_empty()
                || attempt.url().password().is_some()
            {
                attempt.error("Небезопасное перенаправление")
            } else {
                attempt.follow()
            }
        }))
        .build()?)
}

pub fn drive_id_from_input(input: &str) -> Result<String> {
    let input = input.trim();
    if !input.contains("://") {
        validate_drive_id(input)?;
        return Ok(input.to_owned());
    }
    let url = validate_https_url(input)?;
    ensure!(is_drive_host(url.host_str()), "Это не ссылка Google Drive");
    let segments: Vec<_> = url.path_segments().into_iter().flatten().collect();
    ensure!(
        !segments.contains(&"folders"),
        "Нужна ссылка на файл catalog.json, а не на папку Google Drive"
    );
    let from_path = segments
        .windows(3)
        .find(|parts| parts[0] == "file" && parts[1] == "d")
        .map(|parts| parts[2].to_owned());
    let id = from_path
        .or_else(|| {
            url.query_pairs()
                .find(|(key, _)| key == "id")
                .map(|(_, value)| value.into_owned())
        })
        .context("В ссылке Google Drive не найден ID файла")?;
    validate_drive_id(&id)?;
    Ok(id)
}

fn is_drive_host(host: Option<&str>) -> bool {
    matches!(
        host,
        Some("drive.google.com" | "drive.usercontent.google.com")
    )
}

fn is_html(response: &Response) -> bool {
    response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| h.to_ascii_lowercase().contains("text/html"))
}

pub async fn open_url(client: &Client, source: &str) -> Result<Response> {
    let url = validate_https_url(source)?;
    if is_drive_host(url.host_str()) {
        return open_drive(client, &drive_id_from_input(source)?).await;
    }
    let response = client
        .get(url)
        .send()
        .await
        .context("Не удалось соединиться с сервером")?
        .error_for_status()?;
    ensure!(
        !is_html(&response),
        "Сервер вернул HTML-страницу вместо файла. Нужна прямая ссылка на скачивание"
    );
    Ok(response)
}

pub async fn open_drive(client: &Client, file_id: &str) -> Result<Response> {
    validate_drive_id(file_id)?;
    let mut url = Url::parse("https://drive.usercontent.google.com/download")?;
    url.query_pairs_mut()
        .extend_pairs([("id", file_id), ("export", "download"), ("confirm", "t")]);
    let response = client
        .get(url.clone())
        .send()
        .await
        .context("Google Drive недоступен")?
        .error_for_status()?;
    if !is_html(&response) {
        return Ok(response);
    }
    let html = read_limited(response, MAX_CONFIRMATION_BYTES).await?;
    let confirmation = drive_confirmation(&String::from_utf8_lossy(&html), &url, file_id)?;
    let response = client.get(confirmation).send().await?.error_for_status()?;
    ensure!(
        !is_html(&response),
        "Google Drive не отдал файл. Проверьте доступ «Все, у кого есть ссылка»; также мог быть исчерпан лимит скачиваний"
    );
    Ok(response)
}

fn drive_confirmation(html: &str, base: &Url, file_id: &str) -> Result<Url> {
    let document = Html::parse_document(html);
    let forms = Selector::parse("form#download-form").expect("valid CSS selector");
    let inputs = Selector::parse("input[type=hidden]").expect("valid CSS selector");
    let form = document.select(&forms).next().context("Google Drive вернул страницу вместо файла. Откройте доступ «Все, у кого есть ссылка» и проверьте лимит скачиваний")?;
    let action = form
        .value()
        .attr("action")
        .context("Нет адреса подтверждения скачивания")?;
    let mut url = base.join(action)?;
    ensure!(
        url.scheme() == "https"
            && is_drive_host(url.host_str())
            && url.username().is_empty()
            && url.password().is_none(),
        "Неожиданный адрес подтверждения Google Drive"
    );
    ensure!(
        matches!(url.path(), "/download" | "/uc"),
        "Неожиданный путь подтверждения Google Drive"
    );
    let mut values = std::collections::BTreeMap::<String, String>::new();
    for input in form.select(&inputs) {
        if let (Some(name), Some(value)) = (input.value().attr("name"), input.value().attr("value"))
            && matches!(name, "id" | "export" | "confirm" | "uuid" | "at")
        {
            ensure!(
                values.insert(name.to_owned(), value.to_owned()).is_none(),
                "Повтор параметра подтверждения"
            );
        }
    }
    ensure!(
        values.get("id").is_some_and(|id| id == file_id),
        "ID файла изменился при подтверждении"
    );
    ensure!(
        values.contains_key("confirm"),
        "Нет подтверждения скачивания"
    );
    url.query_pairs_mut().clear().extend_pairs(values);
    Ok(url)
}

pub async fn read_limited(response: Response, limit: usize) -> Result<Vec<u8>> {
    if let Some(size) = response.content_length() {
        ensure!(
            size <= limit as u64,
            "Ответ сервера превышает допустимый размер"
        );
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        ensure!(
            bytes.len().saturating_add(chunk.len()) <= limit,
            "Ответ сервера превышает допустимый размер"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn load_catalog(client: &Client, source: &str) -> Result<CatalogDocument> {
    let mut document = resolve_catalog(client, source).await?;
    crate::discovery::apply_manager_availability(&mut document);
    Ok(document)
}

pub(crate) async fn resolve_catalog(client: &Client, source: &str) -> Result<CatalogDocument> {
    let document = tokio::time::timeout(SOURCE_TIMEOUT, load_document(client, source))
        .await
        .context("Каталог не ответил вовремя")??;
    crate::discovery::resolve(client, document).await
}

async fn load_document(client: &Client, source: &str) -> Result<CatalogDocument> {
    let source = source.trim();
    if source.is_empty() {
        return CatalogDocument::builtin();
    }
    if source.starts_with("https://") {
        let response = open_url(client, source).await?;
        let bytes = read_limited(response, MAX_CATALOG_BYTES).await?;
        let catalog = Catalog::parse_with_builtin(&bytes)?;
        for package in catalog.packages.iter().filter(|p| p.enabled) {
            if let Some(artifact) = &package.artifact {
                ensure!(
                    artifact.drive_file_id.is_some() || artifact.url.is_some(),
                    "В публичном каталоге у «{}» есть только локальный путь. Опубликуйте ссылку на установщик",
                    package.name
                );
            }
        }
        return Ok(CatalogDocument {
            catalog,
            local_root: None,
            is_demo: false,
            diagnostics: Default::default(),
        });
    }
    if source.contains("://") {
        bail!("Для сетевого каталога используйте HTTPS-ссылку");
    }
    let mut path = std::path::PathBuf::from(source);
    if tokio::fs::metadata(&path).await.is_ok_and(|m| m.is_dir()) {
        path.push("catalog.json");
    }
    let metadata = tokio::fs::metadata(&path)
        .await
        .with_context(|| format!("Каталог {} недоступен", path.display()))?;
    ensure!(
        metadata.len() <= MAX_CATALOG_BYTES as u64,
        "Каталог превышает 8 МБ"
    );
    use tokio::io::AsyncReadExt;
    let file = tokio::fs::File::open(&path).await?;
    let mut bytes = Vec::new();
    file.take(MAX_CATALOG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .await?;
    ensure!(bytes.len() <= MAX_CATALOG_BYTES, "Каталог превышает 8 МБ");
    let catalog = Catalog::parse_with_builtin(&bytes)?;
    let local_root = path.canonicalize()?.parent().map(|p| p.to_owned());
    Ok(CatalogDocument {
        catalog,
        local_root,
        is_demo: false,
        diagnostics: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "1Abc_def-GHI234567890";

    #[test]
    fn accepts_file_links_but_not_folder_or_lookalike_hosts() {
        for value in [
            ID.to_owned(),
            format!("https://drive.google.com/file/d/{ID}/view?usp=sharing"),
            format!("https://drive.google.com/open?id={ID}"),
        ] {
            assert_eq!(drive_id_from_input(&value).unwrap(), ID);
        }
        for value in [
            "https://drive.google.com/drive/folders/12345678901",
            "https://drive.google.com.evil.test/file/d/12345678901/view",
            "http://drive.google.com/open?id=12345678901",
        ] {
            assert!(drive_id_from_input(value).is_err());
        }
    }

    #[test]
    fn large_file_confirmation_is_bound_to_the_original_file_and_host() {
        let base = Url::parse("https://drive.usercontent.google.com/download").unwrap();
        let form = format!(
            r#"<form id="download-form" action="https://drive.usercontent.google.com/download"><input type="hidden" name="id" value="{ID}"><input type="hidden" name="confirm" value="t"><input type="hidden" name="uuid" value="token&amp;value"></form>"#
        );
        let result = drive_confirmation(&form, &base, ID).unwrap();
        assert!(
            result
                .query_pairs()
                .any(|(k, v)| k == "uuid" && v == "token&value")
        );
        assert!(
            drive_confirmation(
                &form.replace("drive.usercontent.google.com", "evil.test"),
                &base,
                ID
            )
            .is_err()
        );
        assert!(drive_confirmation(&form, &base, "different_file_id").is_err());
    }
}
