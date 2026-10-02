use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, ensure};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::catalog::{CatalogDocument, Package};
use crate::storage::{InstalledPackage, Library, Store};
use crate::transfer::Progress;
use crate::uninstall::InstalledProgram;
use crate::{installer, network, transfer, uninstall};

#[derive(Clone, Debug)]
pub enum JobStatus {
    Queued,
    Downloading,
    Verifying,
    Installing,
    Removing,
    Done { reboot_required: bool },
    Failed(String),
    Cancelled,
    Skipped(String),
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Done { .. } | Self::Failed(_) | Self::Cancelled | Self::Skipped(_)
        )
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Queued => "В очереди",
            Self::Downloading => "Загрузка",
            Self::Verifying => "Проверка файла",
            Self::Installing => "Установка",
            Self::Removing => "Удаление",
            Self::Done {
                reboot_required: false,
            } => "Готово",
            Self::Done {
                reboot_required: true,
            } => "Готово · нужна перезагрузка",
            Self::Failed(_) => "Ошибка",
            Self::Cancelled => "Отменено",
            Self::Skipped(_) => "Пропущено",
        }
    }
}

pub enum WorkerEvent {
    Catalog {
        generation: u64,
        result: Result<CatalogDocument, String>,
    },
    Status {
        id: String,
        status: JobStatus,
    },
    Progress {
        id: String,
        progress: Progress,
    },
    Installed(InstalledPackage),
    Programs {
        generation: u64,
        result: Result<crate::inventory::Inventory, String>,
    },
    Removed {
        id: String,
        managed_ids: Vec<String>,
    },
    Warning(String),
    QueueFinished,
}

pub struct Engine {
    runtime: Runtime,
    client: reqwest::Client,
    sender: Sender<WorkerEvent>,
    catalog_task: Option<JoinHandle<()>>,
    queue_task: Option<JoinHandle<()>>,
    generation: u64,
    cancel: CancellationToken,
    shutdown: CancellationToken,
    inventory_generation: u64,
    inventory_cancel: CancellationToken,
}

impl Engine {
    pub fn new() -> Result<(Self, Receiver<WorkerEvent>)> {
        let (sender, receiver) = mpsc::channel();
        let engine = Self {
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?,
            client: network::client()?,
            sender,
            catalog_task: None,
            queue_task: None,
            generation: 0,
            cancel: CancellationToken::new(),
            shutdown: CancellationToken::new(),
            inventory_generation: 0,
            inventory_cancel: CancellationToken::new(),
        };
        Ok((engine, receiver))
    }

    pub fn load_catalog(&mut self, source: String) -> u64 {
        if let Some(task) = self.catalog_task.take() {
            task.abort();
        }
        self.generation += 1;
        let generation = self.generation;
        let client = self.client.clone();
        let sender = self.sender.clone();
        self.catalog_task = Some(self.runtime.spawn(async move {
            let result = resolve_catalog(&client, &source).await;
            // The receiver is gone only when the window has closed.
            let _ = sender.send(WorkerEvent::Catalog { generation, result });
        }));
        generation
    }

    pub fn start(
        &mut self,
        packages: Vec<Package>,
        document: &CatalogDocument,
        library: Library,
        store: Store,
    ) -> Result<()> {
        ensure!(
            self.queue_task
                .as_ref()
                .is_none_or(|task| task.is_finished()),
            "Очередь уже запущена"
        );
        self.cancel = CancellationToken::new();
        let runner = QueueRunner {
            client: self.client.clone(),
            sender: self.sender.clone(),
            cancel: self.cancel.clone(),
            local_root: document.local_root.clone(),
            store,
        };
        self.queue_task = Some(self.runtime.spawn(async move {
            runner.run(packages, library).await;
        }));
        Ok(())
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn scan_programs(&mut self, library: Library, catalog: crate::catalog::Catalog) -> u64 {
        self.inventory_cancel.cancel();
        self.inventory_cancel = self.shutdown.child_token();
        self.inventory_generation += 1;
        let generation = self.inventory_generation;
        let sender = self.sender.clone();
        let cancel = self.inventory_cancel.clone();
        self.runtime.spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                crate::inventory::scan(&catalog, &library, &cancel)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|result| result.map_err(|e| format!("{e:#}")));
            let _ = sender.send(WorkerEvent::Programs { generation, result });
        });
        generation
    }

    pub fn remove(
        &mut self,
        programs: Vec<InstalledProgram>,
        library: Library,
        catalog: crate::catalog::Catalog,
        store: Store,
    ) -> Result<()> {
        ensure!(
            self.queue_task
                .as_ref()
                .is_none_or(|task| task.is_finished()),
            "Очередь уже запущена"
        );
        self.cancel = CancellationToken::new();
        let runner = QueueRunner {
            client: self.client.clone(),
            sender: self.sender.clone(),
            cancel: self.cancel.clone(),
            local_root: None,
            store,
        };
        self.queue_task = Some(self.runtime.spawn(async move {
            runner.remove(programs, library, catalog).await;
        }));
        Ok(())
    }
    pub fn is_cancelling(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown.cancel();
        self.cancel.cancel();
        if let Some(task) = &self.catalog_task {
            task.abort();
        }
    }
}

/// Loads a catalog, reusing the last successful result while it is fresh.
///
/// This keeps the interface responsive and avoids re-querying every official
/// source on each launch. When a refresh fails, the cached catalog is shown
/// instead of an empty window.
async fn resolve_catalog(
    client: &reqwest::Client,
    source: &str,
) -> Result<CatalogDocument, String> {
    let cached = cache::read(source);
    if let Some((document, age)) = &cached
        && *age <= cache::MAX_AGE
    {
        return Ok(document.clone());
    }
    match network::load_catalog(client, source).await {
        Ok(document) => {
            // A cache write failure only costs an extra refresh next time.
            let _ = cache::write(source, &document);
            Ok(document)
        }
        Err(error) => match cached {
            Some((document, _)) => Ok(document),
            None => Err(format!("{error:#}")),
        },
    }
}

/// On-disk cache of the last resolved catalog document.
mod cache {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use anyhow::Result;
    use sha2::{Digest, Sha256};

    use crate::catalog::CatalogDocument;

    /// Catalog metadata changes slowly, so an hour-old cache is reused as is.
    pub const MAX_AGE: Duration = Duration::from_secs(60 * 60);

    fn location(source: &str) -> Result<PathBuf> {
        let name = hex::encode(Sha256::digest(source.trim().as_bytes()));
        let store = crate::storage::Store::open()?;
        let folder = store.cache_directory();
        std::fs::create_dir_all(&folder)?;
        Ok(folder.join(format!("{name}.json")))
    }

    /// Returns the cached document together with its age.
    pub fn read(source: &str) -> Option<(CatalogDocument, Duration)> {
        let path = location(source).ok()?;
        let document: CatalogDocument = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
        // The file timestamp records when the document was stored.
        let stored = std::fs::metadata(&path).ok()?.modified().ok()?;
        let age = SystemTime::now().duration_since(stored).ok()?;
        Some((document, age))
    }

    pub fn write(source: &str, document: &CatalogDocument) -> Result<()> {
        crate::storage::write_json(&location(source)?, document)
    }
}

struct QueueRunner {
    client: reqwest::Client,
    sender: Sender<WorkerEvent>,
    cancel: CancellationToken,
    local_root: Option<std::path::PathBuf>,
    store: Store,
}

impl QueueRunner {
    fn emit(&self, event: WorkerEvent) {
        if self.sender.send(event).is_err() {
            self.cancel.cancel();
        }
    }

    fn status(&self, id: &str, status: JobStatus) {
        self.emit(WorkerEvent::Status {
            id: id.into(),
            status,
        });
    }

    async fn run(self, packages: Vec<Package>, mut library: Library) {
        let mut failed = HashSet::new();
        for package in packages {
            if self.cancel.is_cancelled() {
                self.status(&package.id, JobStatus::Cancelled);
                continue;
            }
            if let Some(dependency) = package.depends_on.iter().find(|id| failed.contains(*id)) {
                self.status(
                    &package.id,
                    JobStatus::Skipped(format!("Не установлена зависимость {dependency}")),
                );
                failed.insert(package.id.clone());
                continue;
            }
            match self.run_package(&package).await {
                Ok((outcome, sha256)) => {
                    self.record_install(&package, outcome, sha256, &mut library)
                }
                Err(_) if self.cancel.is_cancelled() => {
                    self.status(&package.id, JobStatus::Cancelled)
                }
                Err(error) => {
                    failed.insert(package.id.clone());
                    self.status(&package.id, JobStatus::Failed(format!("{error:#}")));
                }
            }
        }
        self.emit(WorkerEvent::QueueFinished);
    }

    async fn run_package(&self, package: &Package) -> Result<(installer::InstallOutcome, String)> {
        if package.is_managed() {
            self.status(&package.id, JobStatus::Installing);
            let spec = package.install.clone();
            let id = package.winget_id().map(str::to_owned);
            let repository = package.winget_repository();
            let log = self.store.logs.join(format!("install-{}.log", package.id));
            let outcome = tokio::task::spawn_blocking(move || match spec {
                Some(crate::catalog::InstallSpec::Winget) => crate::system::winget::install(
                    id.as_deref()
                        .ok_or_else(|| anyhow::anyhow!("Нет ID WinGet"))?,
                    repository,
                    &log,
                ),
                Some(crate::catalog::InstallSpec::VscodeExtension { extension_id }) => {
                    crate::system::vscode::install(&extension_id, &log)
                }
                _ => anyhow::bail!("Неизвестный менеджер установки"),
            })
            .await??;
            return Ok((outcome, String::new()));
        }
        self.status(&package.id, JobStatus::Downloading);
        let acquired = transfer::acquire(
            &self.client,
            package,
            self.local_root.as_deref(),
            &self.store.cache,
            &self.cancel,
            |progress| {
                self.emit(WorkerEvent::Progress {
                    id: package.id.clone(),
                    progress,
                });
            },
        )
        .await?;
        ensure!(!self.cancel.is_cancelled(), "Отменено");
        if package.requires_signature() {
            let path = acquired.path.clone();
            self.status(&package.id, JobStatus::Verifying);
            tokio::task::spawn_blocking(move || installer::verify_signature(&path)).await??;
        }
        ensure!(!self.cancel.is_cancelled(), "Отменено");
        self.status(&package.id, JobStatus::Installing);
        let outcome =
            installer::install(package, acquired.path, &self.store.logs, &self.cancel).await?;
        Ok((outcome, acquired.sha256))
    }

    fn record_install(
        &self,
        package: &Package,
        outcome: installer::InstallOutcome,
        sha256: String,
        library: &mut Library,
    ) {
        let uninstall = match uninstall::target_for_package(package) {
            Ok(target) => target,
            Err(error) => {
                self.emit(WorkerEvent::Warning(format!(
                    "Не удалось связать запись удаления: {error:#}"
                )));
                None
            }
        };
        let entry = InstalledPackage {
            id: package.id.clone(),
            name: package.name.clone(),
            version: package.version.clone(),
            sha256,
            installed_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default(),
            reboot_required: outcome.reboot_required,
            uninstall,
            externally_detected: false,
        };
        library.insert(entry.id.clone(), entry.clone());
        if let Err(error) = self.store.save_library(library) {
            self.emit(WorkerEvent::Warning(format!(
                "Установка завершена, но историю сохранить не удалось: {error:#}"
            )));
        }
        self.emit(WorkerEvent::Installed(entry));
        self.status(
            &package.id,
            JobStatus::Done {
                reboot_required: outcome.reboot_required,
            },
        );
    }

    async fn remove(
        self,
        programs: Vec<InstalledProgram>,
        mut library: Library,
        catalog: crate::catalog::Catalog,
    ) {
        let mut failed = HashSet::<String>::new();
        for program in programs {
            if self.cancel.is_cancelled() {
                self.status(&program.id, JobStatus::Cancelled);
                continue;
            }
            if failed.iter().any(|id| {
                catalog.package(id).is_some_and(|p| {
                    p.depends_on
                        .iter()
                        .any(|id| program.managed_ids.contains(id))
                })
            }) {
                self.status(
                    &program.id,
                    JobStatus::Skipped("Не удалось удалить зависимое дополнение".into()),
                );
                failed.extend(program.managed_ids);
                continue;
            }
            self.status(&program.id, JobStatus::Removing);
            let target = program.clone();
            let logs = self.store.logs.clone();
            let result =
                tokio::task::spawn_blocking(move || uninstall::remove(&target, &logs)).await;
            match result
                .map_err(anyhow::Error::from)
                .and_then(|result| result)
            {
                Ok(outcome) => {
                    for id in &program.managed_ids {
                        library.remove(id);
                    }
                    if let Err(error) = self.store.save_library(&library) {
                        self.emit(WorkerEvent::Warning(format!(
                            "Не удалось сохранить историю: {error:#}"
                        )));
                    }
                    self.emit(WorkerEvent::Removed {
                        id: program.id.clone(),
                        managed_ids: program.managed_ids,
                    });
                    self.status(
                        &program.id,
                        JobStatus::Done {
                            reboot_required: outcome.reboot_required,
                        },
                    );
                }
                Err(error) => {
                    failed.extend(program.managed_ids);
                    self.status(&program.id, JobStatus::Failed(format!("{error:#}")));
                }
            }
        }
        self.emit(WorkerEvent::QueueFinished);
    }
}
