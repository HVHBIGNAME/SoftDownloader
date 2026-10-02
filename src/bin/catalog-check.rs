use anyhow::{Context, Result, ensure};
use softdownloader::{
    catalog::{Catalog, CatalogDocument},
    installer, network, transfer,
};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    let mut public = false;
    let mut resolve = false;
    let mut file = None;
    let mut download = None;
    let mut cache = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--public" => public = true,
            "--resolve" => resolve = true,
            "--download-only" => download = Some(args.next().context("Missing package ID")?),
            "--cache" => {
                cache = Some(PathBuf::from(
                    args.next().context("Missing cache directory")?,
                ))
            }
            _ => {
                ensure!(
                    file.is_none(),
                    "Usage: catalog-check [--public] [--resolve] [catalog.json] [--download-only ID --cache DIR]"
                );
                file = Some(argument);
            }
        }
    }
    let client = network::client()?;
    let document = if resolve {
        network::load_catalog(&client, file.as_deref().unwrap_or_default()).await?
    } else {
        let path = file.context("Supply catalog.json or --resolve")?;
        let bytes = std::fs::read(&path).with_context(|| format!("Cannot read {path}"))?;
        ensure!(
            bytes.len() <= network::MAX_CATALOG_BYTES,
            "Catalog is larger than 8 MiB"
        );
        CatalogDocument {
            catalog: Catalog::parse_with_builtin(&bytes)?,
            local_root: None,
            is_demo: false,
            diagnostics: Default::default(),
        }
    };
    if public {
        for package in document.catalog.packages.iter().filter(|p| p.enabled) {
            if let Some(artifact) = &package.artifact {
                ensure!(
                    artifact.drive_file_id.is_some() || artifact.url.is_some(),
                    "{} has no public download source",
                    package.id
                );
                ensure!(
                    artifact.local_path.is_none(),
                    "{} exposes a local path in a public catalog",
                    package.id
                );
            }
        }
    }
    for (id, error) in &document.diagnostics {
        eprintln!("{id}: {error}");
    }
    ensure!(
        document.diagnostics.is_empty(),
        "Some official sources could not be resolved"
    );
    println!(
        "OK: {} — {} packages, {} categories",
        document.catalog.title,
        document.catalog.packages.len(),
        document.catalog.categories.len()
    );
    if resolve {
        for package in &document.catalog.packages {
            println!(
                "{}: {} ({})",
                package.id,
                package.version,
                if package.requires_signature() {
                    "Authenticode required"
                } else {
                    "published SHA-256"
                }
            );
        }
    }
    if let Some(id) = download {
        ensure!(resolve, "Use --resolve with --download-only");
        let cache = cache.context("Supply --cache for download-only verification")?;
        std::fs::create_dir_all(&cache)?;
        let package = document.catalog.package(&id).context("Package not found")?;
        let acquired = transfer::acquire(
            &client,
            package,
            document.local_root.as_deref(),
            &cache,
            &CancellationToken::new(),
            |_| {},
        )
        .await?;
        if package.requires_signature() {
            installer::verify_signature(&acquired.path)?;
        }
        println!(
            "Downloaded (not executed): {}\nSHA-256: {}",
            acquired.path.display(),
            acquired.sha256
        );
    }
    Ok(())
}
