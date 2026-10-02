pub mod appx;
pub mod icons;
pub mod vscode;
pub mod winget;

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use tokio_util::sync::CancellationToken;

pub struct Captured {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    command
}

pub fn capture(
    command: &mut Command,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Captured> {
    ensure!(!cancel.is_cancelled(), "Запрос списка программ отменён");
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut child = hidden(command)
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .spawn()?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancel.is_cancelled() || started.elapsed() >= timeout {
            child
                .kill()
                .context("Не удалось остановить запрос списка программ")?;
            child.wait()?;
            bail!("Запрос списка программ отменён или превысил время ожидания");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let read = |file: &mut std::fs::File| -> Result<Vec<u8>> {
        ensure!(
            file.metadata()?.len() <= 8 * 1024 * 1024,
            "Ответ системной утилиты слишком большой"
        );
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 8 * 1024 * 1024,
            "Ответ системной утилиты слишком большой"
        );
        Ok(bytes)
    };
    Ok(Captured {
        status,
        stdout: read(&mut stdout)?,
        stderr: read(&mut stderr)?,
    })
}

pub fn run_logged(command: &mut Command, log: &Path) -> Result<ExitStatus> {
    let output = std::fs::File::create(log)?;
    hidden(command)
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output)
        .status()
        .with_context(|| {
            format!(
                "Не удалось запустить системную утилиту; журнал: {}",
                log.display()
            )
        })
}

pub fn valid_tool_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 180
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
}

#[cfg(windows)]
pub fn open_folder(path: &Path) -> Result<()> {
    let windows = crate::installer::windows::system_directory()?;
    let explorer = windows
        .parent()
        .context("Нет папки Windows")?
        .join("explorer.exe");
    Command::new(explorer).arg(path).spawn()?;
    Ok(())
}

#[cfg(not(windows))]
pub fn open_folder(_path: &Path) -> Result<()> {
    bail!("Открытие папки поддерживается в Windows")
}
