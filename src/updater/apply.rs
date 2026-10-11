use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{ReadyUpdate, UpdateResult};
use crate::storage::{Store, write_json};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    target: PathBuf,
    incoming: PathBuf,
    backup: PathBuf,
    working_directory: PathBuf,
    arguments: Vec<OsString>,
    parent_pid: u32,
    version: String,
    size: u64,
    sha256: String,
    old_sha256: String,
}

#[derive(Deserialize, Serialize)]
struct Health {
    pid: u32,
    sha256: String,
}

pub struct Handoff {
    child: Child,
    directory: PathBuf,
    started: Instant,
    detached: bool,
}

impl Handoff {
    pub fn ready(&mut self) -> Result<bool> {
        if let Some(status) = self.child.try_wait()? {
            let error = std::fs::read_to_string(self.directory.join("error.txt"))
                .unwrap_or_else(|_| format!("Процесс обновления завершился: {status}"));
            anyhow::bail!("{error}");
        }
        ensure!(
            self.started.elapsed() < Duration::from_secs(30),
            "Процесс обновления не подтвердил готовность"
        );
        if self.directory.join("prepared").is_file() {
            self.detached = true;
            return Ok(true);
        }
        Ok(false)
    }
}

impl Drop for Handoff {
    fn drop(&mut self) {
        if !self.detached {
            // Never let an unacknowledged helper update the EXE on a later, unrelated close.
            let _ = self.child.kill();
            let _ = self.child.wait();
            if let Ok(plan) = read_plan(&self.directory)
                && !plan.backup.exists()
            {
                let _ = std::fs::remove_file(plan.incoming);
                let _ = std::fs::remove_dir_all(&self.directory);
            }
        }
    }
}

pub fn launch(update: ReadyUpdate, store: &Store, source: &str) -> Result<Handoff> {
    ensure!(cfg!(windows), "Автообновление поддерживается в Windows");
    let target = std::env::current_exe()?.canonicalize()?;
    let parent = target.parent().context("Нет папки приложения")?;
    let mut incoming = tempfile::Builder::new().prefix(".softdownloader-update-").suffix(".exe")
        .tempfile_in(parent).context("Нет права записи в папку EXE. Переместите приложение в доступную папку и повторите обновление")?;
    std::io::copy(
        &mut std::fs::File::open(update.directory.path().join("SoftDownloader.exe"))?,
        incoming.as_file_mut(),
    )?;
    incoming.flush()?;
    incoming.as_file().sync_all()?;
    verify(
        incoming.path(),
        update.artifact.size,
        &update.artifact.sha256,
    )?;
    let incoming = incoming.into_temp_path();
    let directory = update.directory.path().canonicalize()?;
    let helper = directory.join("runner.exe");
    std::fs::copy(&target, &helper)?;
    let mut arguments = vec![
        OsString::from("--data-dir"),
        store.root.canonicalize()?.into_os_string(),
    ];
    if !source.is_empty() {
        arguments.extend([OsString::from("--catalog"), OsString::from(source)]);
    }
    let plan = Plan {
        backup: incoming.with_extension("backup"),
        incoming: incoming.to_path_buf(),
        old_sha256: digest(&target)?,
        target,
        arguments,
        working_directory: std::env::current_dir()?,
        parent_pid: std::process::id(),
        version: update.version,
        size: update.artifact.size,
        sha256: update.artifact.sha256,
    };
    write_json(&directory.join("plan.json"), &plan)?;
    let child = crate::system::hidden(Command::new(helper).arg("--apply-update").arg(&directory))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Не удалось запустить процесс обновления")?;
    incoming.keep()?;
    let _ = update.directory.keep();
    Ok(Handoff {
        child,
        directory,
        started: Instant::now(),
        detached: false,
    })
}

fn digest(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        hash.update(&buffer[..length]);
    }
    Ok(hex::encode(hash.finalize()))
}

fn verify(path: &Path, size: u64, expected: &str) -> Result<()> {
    ensure!(
        std::fs::metadata(path)?.len() == size && digest(path)? == expected,
        "Проверка SHA-256 перед заменой EXE не пройдена"
    );
    Ok(())
}

fn read_plan(directory: &Path) -> Result<Plan> {
    let bytes = std::fs::read(directory.join("plan.json"))?;
    ensure!(bytes.len() <= 64 * 1024, "Слишком большой план обновления");
    let plan: Plan = serde_json::from_slice(&bytes)?;
    ensure!(
        plan.target.is_absolute() && plan.working_directory.is_absolute(),
        "Нужны абсолютные пути обновления"
    );
    ensure!(
        plan.incoming.parent() == plan.target.parent()
            && plan.backup == plan.incoming.with_extension("backup")
            && plan.incoming != plan.target
            && plan.backup != plan.target,
        "Некорректные пути замены EXE"
    );
    ensure!(
        plan.sha256.len() == 64
            && plan.old_sha256.len() == 64
            && (1024..=super::MAX_EXE_BYTES).contains(&plan.size),
        "Некорректные метаданные обновления"
    );
    Ok(plan)
}

pub fn acknowledge(directory: &Path) -> Result<()> {
    let plan = read_plan(directory)?;
    ensure!(
        std::env::current_exe()?.canonicalize()? == plan.target
            && plan.version == env!("CARGO_PKG_VERSION"),
        "Запущена не ожидаемая версия обновления"
    );
    write_json(
        &directory.join("healthy"),
        &Health {
            pid: std::process::id(),
            sha256: plan.sha256,
        },
    )?;
    Ok(())
}

pub fn apply_update(directory: &Path) -> Result<()> {
    let result = run_update(directory);
    if let Err(error) = &result {
        std::fs::write(directory.join("error.txt"), format!("{error:#}"))?;
    }
    result
}

fn run_update(directory: &Path) -> Result<()> {
    let directory = directory.canonicalize()?;
    ensure!(
        std::env::current_exe()?.canonicalize()?.parent() == Some(directory.as_path()),
        "Процесс обновления запущен вне своей папки"
    );
    let plan = read_plan(&directory)?;
    verify(&plan.incoming, plan.size, &plan.sha256)?;
    super::validate_executable(&plan.incoming)?;
    ensure!(
        digest(&plan.target)? == plan.old_sha256 && !plan.backup.exists(),
        "Приложение уже изменено другим процессом"
    );
    wait_for_parent(plan.parent_pid, &directory)?;
    let result = replace_and_start(&plan, || start_and_check(&plan, &directory));
    let report = UpdateResult {
        version: plan.version.clone(),
        failed_sha256: result.as_ref().err().map(|_| plan.sha256.clone()),
        message: match &result {
            Ok(()) => format!("Обновлено до {}", plan.version),
            Err(error) => format!("Обновление {} отменено: {error:#}", plan.version),
        },
    };
    write_json(
        &directory
            .parent()
            .context("Нет папки обновлений")?
            .join("result.json"),
        &report,
    )?;
    if result.is_err() && digest(&plan.target)? == plan.old_sha256 {
        let child = restart(&plan).context("Не удалось перезапустить прежнюю версию")?;
        std::fs::write(directory.join("restarted-pid"), child.id().to_string())?;
    }
    if result.is_ok() || !plan.backup.exists() {
        if plan.incoming.exists() {
            std::fs::remove_file(&plan.incoming)?;
        }
        std::fs::write(directory.join("complete"), b"done")?;
    }
    result
}

fn replace_and_start(plan: &Plan, start: impl FnOnce() -> Result<()>) -> Result<()> {
    ensure!(
        digest(&plan.target)? == plan.old_sha256,
        "EXE был изменён во время ожидания"
    );
    verify(&plan.incoming, plan.size, &plan.sha256)?;
    replace(&plan.target, &plan.incoming, Some(&plan.backup))?;
    if let Err(error) = start() {
        replace(&plan.target, &plan.backup, None)
            .context("Не удалось вернуть резервную копию EXE")?;
        return Err(error);
    }
    // A backup that is temporarily locked is harmless and can be removed on the next check.
    let _ = std::fs::remove_file(&plan.backup);
    Ok(())
}

fn restart(plan: &Plan) -> Result<Child> {
    Ok(Command::new(&plan.target)
        .args(&plan.arguments)
        .current_dir(&plan.working_directory)
        .spawn()?)
}

fn start_and_check(plan: &Plan, directory: &Path) -> Result<()> {
    let mut child = Command::new(&plan.target)
        .args(&plan.arguments)
        .arg("--update-health")
        .arg(directory)
        .current_dir(&plan.working_directory)
        .spawn()?;
    let result = await_health(&mut child, plan, directory);
    if result.is_err() && child.try_wait()?.is_none() {
        child
            .kill()
            .context("Не удалось остановить неудачно запущенную новую версию")?;
        child.wait()?;
    }
    if result.is_ok() {
        std::fs::write(directory.join("restarted-pid"), child.id().to_string())?;
    }
    result
}

fn await_health(child: &mut Child, plan: &Plan, directory: &Path) -> Result<()> {
    let started = Instant::now();
    loop {
        if std::fs::read(directory.join("healthy"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Health>(&bytes).ok())
            .is_some_and(|health| health.sha256 == plan.sha256 && health.pid == child.id())
        {
            return Ok(());
        }
        ensure!(
            child.try_wait()?.is_none(),
            "Новая версия завершилась до открытия окна"
        );
        if started.elapsed() >= Duration::from_secs(60) {
            anyhow::bail!("Новая версия не подтвердила запуск за 60 секунд");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(windows)]
fn wait_for_parent(pid: u32, directory: &Path) -> Result<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    ensure!(!handle.is_null(), "Не удалось открыть родительский процесс");
    let prepared = std::fs::write(directory.join("prepared"), b"ready");
    let waited = if prepared.is_ok() {
        unsafe { WaitForSingleObject(handle, 120_000) }
    } else {
        u32::MAX
    };
    unsafe {
        CloseHandle(handle);
    }
    prepared?;
    ensure!(
        waited == WAIT_OBJECT_0,
        "Прежняя версия не завершила работу; EXE не заменён"
    );
    Ok(())
}

#[cfg(not(windows))]
fn wait_for_parent(_pid: u32, _directory: &Path) -> Result<()> {
    anyhow::bail!("Автообновление поддерживается в Windows")
}

#[cfg(windows)]
fn replace(target: &Path, incoming: &Path, backup: Option<&Path>) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let target = wide(target);
    let incoming = wide(incoming);
    let backup = backup.map(wide);
    for attempt in 0..50 {
        let ok = unsafe {
            ReplaceFileW(
                target.as_ptr(),
                incoming.as_ptr(),
                backup
                    .as_ref()
                    .map_or(std::ptr::null(), |path| path.as_ptr()),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if ok != 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if attempt == 49 {
            return Err(error)
                .context("Не удалось заменить EXE; возможно, открыта другая копия приложения");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    unreachable!()
}

#[cfg(not(windows))]
fn replace(_target: &Path, _incoming: &Path, _backup: Option<&Path>) -> Result<()> {
    anyhow::bail!("Автообновление поддерживается в Windows")
}

pub(super) fn cleanup_completed(updates: &Path) {
    let Ok(entries) = std::fs::read_dir(updates) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with("update-")
            && entry.file_type().is_ok_and(|kind| kind.is_dir())
            && entry.path().join("complete").is_file()
        {
            if let Ok(plan) = read_plan(&entry.path())
                && plan.backup.exists()
                && (!digest(&plan.target).is_ok_and(|hash| hash == plan.sha256)
                    || std::fs::remove_file(&plan.backup).is_err())
            {
                continue;
            }
            // A just-finished helper can still have its executable open. Retry on the next check.
            if std::fs::remove_dir_all(entry.path()).is_err() && entry.path().is_dir() {
                let _ = std::fs::write(entry.path().join("complete"), b"retry cleanup");
            }
        }
    }
}

#[cfg(test)]
mod tests;
