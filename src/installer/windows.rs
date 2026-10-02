use std::ffi::{OsStr, OsString};
use std::os::windows::{
    ffi::{OsStrExt, OsStringExt},
    process::CommandExt,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0},
    System::{
        SystemInformation::GetSystemDirectoryW,
        Threading::{CREATE_NO_WINDOW, GetExitCodeProcess, INFINITE, WaitForSingleObject},
    },
    UI::{
        Shell::{SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
        WindowsAndMessaging::SW_SHOWNORMAL,
    },
};

use super::InstallOutcome;
use crate::catalog::InstallSpec;

pub fn install(path: &Path, id: &str, spec: &InstallSpec, logs: &Path) -> Result<InstallOutcome> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let log_path = logs.join(format!("{id}-{timestamp}.log"));
    let (executable, arguments) = match spec {
        InstallSpec::Exe { silent_args, .. } => (path.to_owned(), silent_args.clone()),
        InstallSpec::Msi { arguments, .. } => {
            let mut args = vec![
                "/i".into(),
                path.to_string_lossy().into_owned(),
                "/qn".into(),
                "/norestart".into(),
                "/L*v".into(),
                log_path.to_string_lossy().into_owned(),
            ];
            args.extend(arguments.iter().cloned());
            (system_directory()?.join("msiexec.exe"), args)
        }
        InstallSpec::Zip { .. } => bail!("ZIP-пакет не является нативным установщиком"),
    };
    launch(&executable, &arguments, spec.requires_admin(), &log_path)
}

pub(crate) fn launch(
    executable: &Path,
    arguments: &[String],
    admin: bool,
    log_path: &Path,
) -> Result<InstallOutcome> {
    let code = if admin {
        elevated(executable, arguments)?
    } else {
        let output_path = log_path.with_extension("stdout.log");
        let output = std::fs::File::create(output_path)?;
        let status = Command::new(executable)
            .args(arguments)
            .current_dir(executable.parent().context("Нет папки исполняемого файла")?)
            .stdin(Stdio::null())
            .stdout(output.try_clone()?)
            .stderr(output)
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .context("Не удалось запустить установщик. Если он требует UAC, включите requires_admin в каталоге")?;
        status
            .code()
            .context("Установщик завершён без кода возврата")? as u32
    };
    match code {
        0 => Ok(InstallOutcome::default()),
        1641 | 3010 => Ok(InstallOutcome {
            reboot_required: true,
        }),
        1602 => bail!("Установка отменена пользователем"),
        1618 => {
            bail!("Windows уже выполняет другую MSI-установку. Дождитесь её окончания и повторите")
        }
        _ => bail!(
            "Установщик завершился с кодом {code}. Логи: {}",
            log_path.display()
        ),
    }
}

pub(crate) fn system_directory() -> Result<PathBuf> {
    let mut buffer = vec![0_u16; 32768];
    // The buffer is writable and its capacity is passed in UTF-16 code units.
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    ensure!(
        length > 0 && length < buffer.len(),
        "Не удалось определить системную папку Windows"
    );
    Ok(PathBuf::from(OsString::from_wide(&buffer[..length])))
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

fn elevated(executable: &Path, arguments: &[String]) -> Result<u32> {
    let executable = wide(executable.as_os_str());
    let verb = wide(OsStr::new("runas"));
    let arguments = arguments
        .iter()
        .map(|a| quote_argument(a))
        .collect::<Vec<_>>()
        .join(" ");
    let arguments = wide(OsStr::new(&arguments));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: verb.as_ptr(),
        lpFile: executable.as_ptr(),
        lpParameters: arguments.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // UTF-16 strings remain alive for ShellExecuteExW; the returned handle is owned below.
    let started = unsafe { ShellExecuteExW(&mut info) };
    if started == 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(1223) {
            bail!("Запрос прав администратора отменён");
        }
        return Err(error).context("Не удалось запустить установку с правами администратора");
    }
    ensure!(
        !info.hProcess.is_null(),
        "Windows не вернула процесс установщика"
    );
    let handle = ProcessHandle(info.hProcess);
    // The owned process handle remains valid until the wait and exit-code query complete.
    let wait = unsafe { WaitForSingleObject(handle.0, INFINITE) };
    if wait == WAIT_FAILED {
        return Err(std::io::Error::last_os_error().into());
    }
    ensure!(
        wait == WAIT_OBJECT_0,
        "Неожиданный результат ожидания установщика"
    );
    let mut exit_code = 0;
    // exit_code points to an initialized u32 and the handle is still open.
    ensure!(
        unsafe { GetExitCodeProcess(handle.0, &mut exit_code) } != 0,
        "Не удалось получить код установщика"
    );
    Ok(exit_code)
}

struct ProcessHandle(HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // This is the single owner of the handle returned by ShellExecuteExW.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn quote_argument(argument: &str) -> String {
    let mut result = String::from("\"");
    let mut slashes = 0;
    for character in argument.chars() {
        if character == '\\' {
            slashes += 1;
            continue;
        }
        let count = if character == '"' {
            slashes * 2 + 1
        } else {
            slashes
        };
        result.extend(std::iter::repeat_n('\\', count));
        slashes = 0;
        result.push(character);
    }
    result.extend(std::iter::repeat_n('\\', slashes * 2));
    result.push('"');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevated_arguments_round_trip_through_windows_parser() {
        use windows_sys::Win32::{Foundation::LocalFree, UI::Shell::CommandLineToArgvW};
        let expected = [
            "",
            "hello world",
            r#"C:\path with space\"#,
            "a\"b",
            "x\\\"y",
            "кириллица",
            "/S",
            "a & b",
        ];
        let line = format!(
            "program.exe {}",
            expected
                .iter()
                .map(|a| quote_argument(a))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let encoded = wide(OsStr::new(&line));
        let mut count = 0;
        // The command line is NUL-terminated; the allocation is freed after reading.
        unsafe {
            let values = CommandLineToArgvW(encoded.as_ptr(), &mut count);
            assert!(!values.is_null());
            assert_eq!(count as usize, expected.len() + 1);
            for (index, expected) in expected.iter().enumerate() {
                let value = *values.add(index + 1);
                let length = (0..).take_while(|&i| *value.add(i) != 0).count();
                assert_eq!(
                    String::from_utf16_lossy(std::slice::from_raw_parts(value, length)),
                    *expected
                );
            }
            LocalFree(values.cast());
        }
    }
}
