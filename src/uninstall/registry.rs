use std::collections::HashSet;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use windows_sys::Win32::{
    Foundation::LocalFree, System::Environment::ExpandEnvironmentStringsW,
    UI::Shell::CommandLineToArgvW,
};
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY},
};

use super::{Hive, InstalledProgram, UninstallTarget};
use crate::installer::{InstallOutcome, windows};

const UNINSTALL_ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

pub(super) fn scan() -> Result<Vec<InstalledProgram>> {
    let mut programs = Vec::new();
    let mut seen = HashSet::new();
    for hive in [Hive::CurrentUser, Hive::LocalMachine] {
        for is_64bit in [true, false] {
            let root = match open(hive, UNINSTALL_ROOT, is_64bit) {
                Ok(root) => root,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                    ) =>
                {
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            for name in root.enum_keys() {
                let name = name?;
                let key = match root.open_subkey(&name) {
                    Ok(key) => key,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                        ) =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                if let Some(program) = read_program(&key, hive, is_64bit, &name) {
                    let command = string(&key, "QuietUninstallString")
                        .or_else(|| string(&key, "UninstallString"))
                        .unwrap_or_default();
                    if seen.insert((program.name.clone(), program.version.clone(), command)) {
                        programs.push(program);
                    }
                }
            }
        }
    }
    Ok(programs)
}

fn read_program(
    key: &RegKey,
    hive: Hive,
    is_64bit: bool,
    subkey: &str,
) -> Option<InstalledProgram> {
    if key
        .get_value::<u32, _>("SystemComponent")
        .unwrap_or_default()
        == 1
        || string(key, "ParentKeyName").is_some()
    {
        return None;
    }
    let name = string(key, "DisplayName")?;
    if name.trim().is_empty() {
        return None;
    }
    let icon_path = icon_file(key);
    let quiet = string(key, "QuietUninstallString");
    let normal = string(key, "UninstallString");
    if quiet.is_none() && normal.is_none() {
        return None;
    }
    let is_msi = key
        .get_value::<u32, _>("WindowsInstaller")
        .unwrap_or_default()
        == 1
        || normal
            .as_ref()
            .is_some_and(|command| command.to_ascii_lowercase().contains("msiexec"));
    let target = UninstallTarget::Registry {
        hive,
        key: format!(r"{UNINSTALL_ROOT}\{subkey}"),
        is_64bit,
    };
    Some(InstalledProgram {
        id: target.id(),
        name,
        version: string(key, "DisplayVersion").unwrap_or_default(),
        publisher: string(key, "Publisher").unwrap_or_default(),
        quiet: quiet.is_some() || is_msi,
        target,
        managed_ids: Vec::new(),
        package_ids: Vec::new(),
        icon_path,
    })
}

/// Resolves the executable behind `DisplayIcon` or `InstallLocation`.
///
/// Windows stores icon references as a quoted path with an optional resource
/// index, so the value is normalized and only accepted when it is a real file.
fn icon_file(key: &RegKey) -> Option<PathBuf> {
    let candidates = [string(key, "DisplayIcon"), string(key, "InstallLocation")];
    for candidate in candidates.into_iter().flatten() {
        let Ok(path) = expand_environment(icon_reference_path(&candidate)) else {
            continue;
        };
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
        if let Some(found) = crate::system::icons::primary_file(&path) {
            return Some(found);
        }
    }
    None
}

fn icon_reference_path(reference: &str) -> &str {
    let reference = reference.trim();
    let path = match reference.rsplit_once(',') {
        Some((head, index)) if index.trim().parse::<i32>().is_ok() => head,
        _ => reference,
    };
    path.trim().trim_matches('"')
}

fn open(hive: Hive, path: &str, is_64bit: bool) -> std::io::Result<RegKey> {
    let root = RegKey::predef(match hive {
        Hive::CurrentUser => HKEY_CURRENT_USER,
        Hive::LocalMachine => HKEY_LOCAL_MACHINE,
    });
    root.open_subkey_with_flags(
        path,
        KEY_READ
            | if is_64bit {
                KEY_WOW64_64KEY
            } else {
                KEY_WOW64_32KEY
            },
    )
}

fn string(key: &RegKey, name: &str) -> Option<String> {
    key.get_value::<String, _>(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

pub(super) fn remove(program: &InstalledProgram, logs: &Path) -> Result<InstallOutcome> {
    let UninstallTarget::Registry {
        hive,
        key,
        is_64bit,
    } = &program.target
    else {
        bail!("Ожидалась программа из реестра");
    };
    ensure!(
        key.starts_with(&format!("{UNINSTALL_ROOT}\\")),
        "Недопустимый ключ удаления"
    );
    let entry = open(*hive, key, *is_64bit)
        .context("Программа больше не зарегистрирована. Обновите список")?;
    let command = string(&entry, "QuietUninstallString")
        .or_else(|| string(&entry, "UninstallString"))
        .context("Нет команды удаления")?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let log = logs.join(format!("uninstall-{}-{timestamp}.log", &program.id[..12]));
    let (executable, args) = uninstall_command(&command, key, &log)?;
    drop(entry);
    let outcome = windows::launch(&executable, &args, matches!(hive, Hive::LocalMachine), &log)?;
    for _ in 0..60 {
        match open(*hive, key, *is_64bit) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(outcome),
            Err(error) => return Err(error.into()),
            Ok(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
    ensure!(
        outcome.reboot_required,
        "Мастер завершился, но программа всё ещё зарегистрирована. Удаление могло быть отменено"
    );
    Ok(outcome)
}

fn uninstall_command(command: &str, key: &str, log: &Path) -> Result<(PathBuf, Vec<String>)> {
    let expanded = expand_environment(command)?;
    let tokens = split_command_line(&quote_unquoted_executable(&expanded))?;
    let executable = tokens.first().context("Пустая команда удаления")?;
    let file = Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if file.eq_ignore_ascii_case("msiexec.exe") || file.eq_ignore_ascii_case("msiexec") {
        let guid =
            Regex::new(r"(?i)\{[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\}")?;
        let product = guid
            .find(key)
            .or_else(|| guid.find(&expanded))
            .context("Не найден MSI ProductCode")?
            .as_str();
        return Ok((
            windows::system_directory()?.join("msiexec.exe"),
            vec![
                "/x".into(),
                product.into(),
                "/qn".into(),
                "/norestart".into(),
                "/L*v".into(),
                log.to_string_lossy().into_owned(),
            ],
        ));
    }
    let executable = PathBuf::from(executable);
    ensure!(
        executable.is_absolute() && executable.is_file(),
        "Путь деинсталлятора недоступен: {}",
        executable.display()
    );
    Ok((executable, tokens.into_iter().skip(1).collect()))
}

fn quote_unquoted_executable(command: &str) -> String {
    let command = command.trim();
    if !command.starts_with('"') {
        for (index, _) in command.to_ascii_lowercase().match_indices(".exe") {
            let prefix = &command[..index + 4];
            if Path::new(prefix).is_absolute() && Path::new(prefix).is_file() {
                return format!("\"{prefix}\"{}", &command[index + 4..]);
            }
        }
    }
    command.to_owned()
}

fn expand_environment(value: &str) -> Result<String> {
    ensure!(!value.contains('\0'), "NUL в команде удаления");
    let input: Vec<u16> = std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut output = vec![0_u16; 32768];
    // Input is NUL-terminated and output length is specified in UTF-16 units.
    let length = unsafe {
        ExpandEnvironmentStringsW(input.as_ptr(), output.as_mut_ptr(), output.len() as u32)
    } as usize;
    ensure!(
        length > 0 && length <= output.len(),
        "Не удалось раскрыть переменные в команде удаления"
    );
    String::from_utf16(&output[..length - 1]).context("Некорректная строка команды")
}

fn split_command_line(command: &str) -> Result<Vec<String>> {
    let encoded: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
    let mut count = 0;
    // CommandLineToArgvW returns one allocation containing pointers and strings.
    let values = unsafe { CommandLineToArgvW(encoded.as_ptr(), &mut count) };
    ensure!(!values.is_null(), "Не удалось разобрать команду удаления");
    let mut result = Vec::new();
    // Every returned argument is NUL-terminated and readable until LocalFree.
    unsafe {
        for index in 0..count as usize {
            let value = *values.add(index);
            let length = (0..).take_while(|&i| *value.add(i) != 0).count();
            result.push(String::from_utf16_lossy(std::slice::from_raw_parts(
                value, length,
            )));
        }
        LocalFree(values.cast());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_references_allow_spaces_commas_quotes_and_negative_indices() {
        assert_eq!(
            icon_reference_path(r#" "C:\Program Files\App, Inc\app.exe",-42 "#),
            r"C:\Program Files\App, Inc\app.exe"
        );
        assert_eq!(
            icon_reference_path(r#""C:\App\app.exe""#),
            r"C:\App\app.exe"
        );
        assert_eq!(
            icon_reference_path(r"C:\App, Inc\app.exe,0"),
            r"C:\App, Inc\app.exe"
        );
    }

    #[test]
    fn converts_msi_repair_registration_to_quiet_removal() {
        let guid = "{12345678-1234-1234-1234-123456789ABC}";
        let (executable, args) = uninstall_command(
            &format!("MsiExec.exe /I{guid}"),
            guid,
            Path::new(r"C:\temp\uninstall.log"),
        )
        .unwrap();
        assert!(executable.ends_with("msiexec.exe"));
        assert_eq!(&args[..4], &["/x", guid, "/qn", "/norestart"]);
    }

    #[test]
    fn splits_registry_arguments_without_a_shell() {
        let tokens = split_command_line(
            r#""C:\Program Files\App\uninstall.exe" /quiet "argument with spaces""#,
        )
        .unwrap();
        assert_eq!(
            tokens,
            [
                r"C:\Program Files\App\uninstall.exe",
                "/quiet",
                "argument with spaces"
            ]
        );
    }
}
