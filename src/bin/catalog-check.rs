use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use softdownloader::{
    catalog::{Catalog, CatalogDocument, InstallSpec, Package},
    installer, inventory, network,
    storage::Store,
    system::winget,
    transfer,
};
use tokio_util::sync::CancellationToken;

const USAGE: &str = "catalog-check [catalog.json] [--public] [--resolve] [--check-winget] [--inventory [--data-dir DIR]] [--download-only ID --cache DIR] [--video FILE --frame PNG]";

#[derive(Default)]
struct Options {
    public: bool,
    resolve: bool,
    check_winget: bool,
    inventory: bool,
    source: Option<String>,
    download: Option<String>,
    cache: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    video: Option<PathBuf>,
    frame: Option<PathBuf>,
}

impl Options {
    fn parse() -> Result<Option<Self>> {
        let mut options = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--help" | "-h" => {
                    println!("{USAGE}");
                    return Ok(None);
                }
                "--public" => options.public = true,
                "--resolve" => options.resolve = true,
                "--check-winget" => options.check_winget = true,
                "--inventory" => options.inventory = true,
                "--video" => {
                    options.video = Some(PathBuf::from(args.next().context("Missing video path")?))
                }
                "--frame" => {
                    options.frame = Some(PathBuf::from(
                        args.next().context("Missing PNG output path")?,
                    ))
                }
                "--download-only" => {
                    options.download = Some(args.next().context("Missing package ID")?)
                }
                "--cache" => {
                    options.cache = Some(PathBuf::from(
                        args.next().context("Missing cache directory")?,
                    ))
                }
                "--data-dir" => {
                    options.data_dir = Some(PathBuf::from(
                        args.next().context("Missing data directory")?,
                    ))
                }
                _ => {
                    ensure!(
                        !argument.starts_with('-') && options.source.is_none(),
                        "{USAGE}"
                    );
                    options.source = Some(argument);
                }
            }
        }
        ensure!(
            options.download.is_none() || (options.resolve && options.cache.is_some()),
            "Use --resolve and --cache with --download-only"
        );
        Ok(Some(options))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(options) = Options::parse()? else {
        return Ok(());
    };
    if let Some(video) = &options.video {
        return inspect_video(video, options.frame.as_deref());
    }
    let client = network::client()?;
    let document = load_document(&client, &options).await?;
    if options.public {
        check_public(&document)?;
    }
    for (id, error) in &document.diagnostics {
        eprintln!("{id}: {error}");
    }
    if options.download.is_none() {
        ensure!(
            document.diagnostics.is_empty(),
            "Some official sources could not be resolved"
        );
    }
    println!(
        "OK: {} — {} packages, {} categories",
        document.catalog.title,
        document.catalog.packages.len(),
        document.catalog.categories.len()
    );
    if options.resolve {
        for package in &document.catalog.packages {
            println!(
                "{}: {} ({})",
                package.id,
                package.version,
                verification_method(package)
            );
        }
    }
    if options.check_winget {
        check_winget(&document)?;
    }
    if options.inventory {
        inspect_inventory(&document, &options)?;
    }
    if let Some(id) = &options.download {
        download_only(
            &client,
            &document,
            id,
            options.cache.as_ref().context("Missing cache directory")?,
        )
        .await?;
    }
    Ok(())
}

fn inspect_video(path: &std::path::Path, snapshot: Option<&std::path::Path>) -> Result<()> {
    use eframe::icon_data::IconDataExt;
    let mut decoder = softdownloader::video::Decoder::open(&path.canonicalize()?)?;
    let first = decoder.next_frame()?.context("Video contains no frames")?;
    println!(
        "Native Windows video: {}x{}, first timestamp {:?}",
        first.size[0], first.size[1], first.timestamp
    );
    if let Some(path) = snapshot {
        ensure!(
            path.parent().is_some_and(|parent| parent.is_dir()),
            "Snapshot parent must exist"
        );
        let image = eframe::egui::IconData {
            rgba: first.rgba.clone(),
            width: first.size[0] as u32,
            height: first.size[1] as u32,
        };
        std::fs::write(path, image.to_png_bytes().map_err(anyhow::Error::msg)?)?;
    }
    let mut count = 1;
    let mut previous = first.timestamp;
    for _ in 0..329 {
        let Some(frame) = decoder.next_frame()? else {
            break;
        };
        ensure!(
            frame.timestamp >= previous,
            "Non-monotonic video timestamps"
        );
        previous = frame.timestamp;
        count += 1;
    }
    decoder.rewind()?;
    let repeated = decoder.next_frame()?.context("Rewinding failed")?;
    ensure!(
        repeated.rgba == first.rgba && repeated.timestamp == first.timestamp,
        "Rewound frame differs"
    );
    println!("Decoded {count} frames, exact rewind verified; no audio stream selected");
    Ok(())
}

async fn load_document(client: &reqwest::Client, options: &Options) -> Result<CatalogDocument> {
    if options.resolve {
        return network::load_catalog(client, options.source.as_deref().unwrap_or_default()).await;
    }
    let Some(path) = &options.source else {
        return CatalogDocument::builtin();
    };
    ensure!(
        std::fs::metadata(path)?.len() <= network::MAX_CATALOG_BYTES as u64,
        "Catalog is larger than 8 MiB"
    );
    let bytes = std::fs::read(path).with_context(|| format!("Cannot read {path}"))?;
    Ok(CatalogDocument {
        catalog: Catalog::parse_with_builtin(&bytes)?,
        local_root: None,
        is_demo: false,
        diagnostics: Default::default(),
    })
}

fn check_public(document: &CatalogDocument) -> Result<()> {
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
    Ok(())
}

fn verification_method(package: &Package) -> &'static str {
    if package.is_manual() {
        return "manual source";
    }
    match package.install {
        Some(InstallSpec::Winget) => "WinGet manifest and installer verification",
        Some(InstallSpec::VscodeExtension { .. }) => "VS Code Marketplace",
        _ if package.requires_signature() => "Authenticode required",
        _ => "published SHA-256",
    }
}

fn check_winget(document: &CatalogDocument) -> Result<()> {
    let cancel = CancellationToken::new();
    let mut count = 0;
    let mut failures = Vec::new();
    for (id, repository) in document.catalog.packages.iter().filter_map(|package| {
        package
            .winget_id()
            .map(|id| (id, package.winget_repository()))
    }) {
        match winget::verify(id, repository, &cancel) {
            Ok(()) => {
                println!("WinGet OK: {id}");
                count += 1;
            }
            Err(error) => {
                eprintln!("WinGet FAILED: {id}: {error:#}");
                failures.push(id);
            }
        }
    }
    ensure!(
        failures.is_empty(),
        "Unverified WinGet packages: {}",
        failures.join(", ")
    );
    println!("Verified {count} exact WinGet IDs; no installers executed.");
    Ok(())
}

fn inspect_inventory(document: &CatalogDocument, options: &Options) -> Result<()> {
    let store = match &options.data_dir {
        Some(path) => Store::at(path.clone())?,
        None => Store::open()?,
    };
    let library = store.load_library()?;
    let started = Instant::now();
    let result = inventory::scan(&document.catalog, &library, &CancellationToken::new())?;
    let state = inventory::installation_state(&document.catalog, &library, &result.programs);
    println!(
        "Inventory: {} programs, {} catalog matches, {} warnings ({:.1}s)",
        result.programs.len(),
        state.len(),
        result.warnings.len(),
        started.elapsed().as_secs_f32()
    );
    for (id, entry) in &state {
        println!("Installed: {id}: {}", entry.version);
    }
    for warning in &result.warnings {
        eprintln!("Inventory warning: {warning}");
    }
    Ok(())
}

async fn download_only(
    client: &reqwest::Client,
    document: &CatalogDocument,
    id: &str,
    cache: &std::path::Path,
) -> Result<()> {
    let package = document.catalog.package(id).context("Package not found")?;
    ensure!(
        !package.is_managed() && !package.is_manual(),
        "This package is installed by a manager or through its website; no direct artifact"
    );
    if let Some(error) = document.diagnostics.get(id) {
        anyhow::bail!("{id}: {error}");
    }
    std::fs::create_dir_all(cache)?;
    let acquired = transfer::acquire(
        client,
        package,
        document.local_root.as_deref(),
        cache,
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
    Ok(())
}
