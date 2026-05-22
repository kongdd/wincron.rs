use anyhow::{Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub fn pid_path() -> PathBuf {
    crate::log_dir().join("daemon.pid")
}

pub fn write_pid(pid: u32) -> Result<()> {
    let path = pid_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create dir: {}", parent.display()))?;
    }
    fs::write(&path, pid.to_string())
        .with_context(|| format!("failed to write pid file: {}", path.display()))?;
    Ok(())
}

pub fn read_pid() -> Option<u32> {
    fs::read_to_string(pid_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

pub fn remove_pid() {
    let _ = fs::remove_file(pid_path());
}

pub fn config_path() -> PathBuf {
    crate::log_dir().join("cron_paths.txt")
}

pub fn load_paths() -> Result<Vec<PathBuf>> {
    let path = config_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect())
}

pub fn save_paths(paths: &[PathBuf]) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create dir: {}", parent.display()))?;
    }
    let text = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn add_path(new_path: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(new_path)
        .with_context(|| format!("cannot find: {}", new_path.display()))?;
    let mut paths = load_paths()?;
    if !paths.contains(&canonical) {
        paths.push(canonical.clone());
        save_paths(&paths)?;
    }
    Ok(canonical)
}

pub fn delete_path(target: &Path) -> Result<bool> {
    let absolute = fs::canonicalize(target).unwrap_or_else(|_| {
        if target.is_absolute() {
            target.to_path_buf()
        } else {
            env::current_dir().unwrap_or_default().join(target)
        }
    });
    let mut paths = load_paths()?;
    let before = paths.len();
    paths.retain(|p| p != &absolute);
    if paths.len() < before {
        save_paths(&paths)?;
        Ok(true)
    } else {
        Ok(false)
    }
}
