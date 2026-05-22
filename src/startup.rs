use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[derive(Debug)]
pub struct StartupItem {
    pub name: String,
    pub command: String,
    pub cron_path: Option<PathBuf>,
}

#[cfg(windows)]
pub fn install_startup(name: &str, cron_path: Option<&Path>) -> Result<()> {
    use crate::log_dir;
    use std::fs;
    use tracing::info;
    use winreg::enums::*;
    use winreg::RegKey;

    let exe = std::env::current_exe().context("failed to get current exe path")?;

    let command = match cron_path {
        Some(path) => {
            let cron_abs = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            format!(
                "\"{}\" run --cron \"{}\"",
                exe.display(),
                cron_abs.display()
            )
        }
        None => format!("\"{}\" daemon-run", exe.display()),
    };

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run_key, _) = hkcu
        .create_subkey(RUN_KEY)
        .context("failed to open registry Run key")?;

    run_key
        .set_value(name, &command)
        .context("failed to set startup registry value")?;

    info!("installed startup item: {}", name);
    info!("startup command: {}", command);

    println!("Installed startup item: {name}");
    println!("Command : {command}");
    println!("Log dir : {}", log_dir().display());

    Ok(())
}

#[cfg(not(windows))]
pub fn install_startup(_name: &str, _cron_path: Option<&Path>) -> Result<()> {
    Err(anyhow::anyhow!(
        "install-startup is only supported on Windows"
    ))
}

#[cfg(windows)]
pub fn uninstall_startup(name: &str) -> Result<()> {
    use tracing::info;
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
        .context("failed to open registry Run key")?;

    match run_key.delete_value(name) {
        Ok(_) => {
            info!("removed startup item: {}", name);
            println!("Removed startup item: {name}");
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("Startup item not found: {name}");
        }
        Err(e) => {
            return Err(e).context("failed to delete startup registry value");
        }
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn uninstall_startup(_name: &str) -> Result<()> {
    Err(anyhow::anyhow!(
        "uninstall-startup is only supported on Windows"
    ))
}

#[cfg(windows)]
pub fn list_startup_items() -> Result<Vec<StartupItem>> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(RUN_KEY, KEY_READ)
        .context("failed to open registry Run key")?;

    let mut items = Vec::new();

    for value in run_key.enum_values() {
        let (name, _) = value?;

        let Ok(command) = run_key.get_value::<String, _>(&name) else {
            continue;
        };

        let command_lc = command.to_lowercase();

        if command_lc.contains("wincron") && command_lc.contains(" run ") {
            items.push(StartupItem {
                name,
                cron_path: extract_cron_path(&command),
                command,
            });
        }
    }

    Ok(items)
}

#[cfg(not(windows))]
pub fn list_startup_items() -> Result<Vec<StartupItem>> {
    Ok(Vec::new())
}

/// Extract the path following --cron from a startup command line.
fn extract_cron_path(command: &str) -> Option<PathBuf> {
    let mut iter = split_command_like_windows(command).into_iter();

    while let Some(arg) = iter.next() {
        if arg == "--cron" {
            return iter.next().map(Into::into);
        }

        if let Some(rest) = arg.strip_prefix("--cron=") {
            return Some(PathBuf::from(rest));
        }
    }

    None
}

fn split_command_like_windows(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in s.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}
