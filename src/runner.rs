use crate::{log_dir, tasks::load_tasks};
use anyhow::{anyhow, Context, Result};
use chrono::{Local, Utc};
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};
use tracing::{error, info};

pub fn run_scheduler(cron_path: PathBuf, tick_seconds: u64) -> Result<()> {
    let cron_path = fs::canonicalize(&cron_path)
        .with_context(|| format!("cannot find cron file: {}", cron_path.display()))?;

    info!("wincron started");
    info!("cron file: {}", cron_path.display());
    info!("log dir: {}", log_dir().display());

    let task_dir = cron_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let mut last_modified = None;
    let mut tasks = Vec::new();

    loop {
        let modified = fs::metadata(&cron_path).and_then(|m| m.modified()).ok();

        if modified != last_modified {
            match load_tasks(&cron_path) {
                Ok(new_tasks) => {
                    info!("loaded {} task(s)", new_tasks.len());
                    tasks = new_tasks;
                    last_modified = modified;
                }
                Err(e) => {
                    error!("failed to load cron file: {e:?}");
                }
            }
        }

        let now = Utc::now();

        for task in tasks.iter_mut() {
            if task.next_run.is_none() {
                task.next_run = task.schedule.upcoming(Utc).next();
            }

            let Some(next) = task.next_run else {
                continue;
            };

            if now < next {
                continue;
            }

            let line_no = task.line_no;
            let command = task.command.clone();
            let cwd = task_dir.clone();

            info!("dispatch line {}: {}", line_no, command);

            thread::spawn(move || {
                if let Err(e) = run_shell_command(&command, &cwd) {
                    error!("failed to run line {}: {:?}", line_no, e);
                }
            });

            task.next_run = task.schedule.upcoming(Utc).next();
        }

        thread::sleep(Duration::from_secs(tick_seconds.max(1)));
    }
}

fn run_shell_command(command: &str, cwd: &Path) -> Result<()> {
    let started_at = Local::now();

    info!("task started at {}", started_at.format("%Y-%m-%d %H:%M:%S"));
    info!("task command: {}", command);
    info!("task cwd: {}", cwd.display());

    #[cfg(windows)]
    let command = format!("pushd \"{}\" && {}", cmd_path(cwd), command);

    #[cfg(windows)]
    let output = Command::new("cmd")
        .arg("/S")
        .arg("/C")
        .raw_arg(&command)
        .current_dir(windows_shell_start_dir())
        .output()
        .with_context(|| format!("failed to run command: {command}"))?;

    #[cfg(not(windows))]
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to run command: {command}"))?;

    info!(
        "task finished at {}",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    info!("exit status: {}", output.status);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.trim().is_empty() {
        info!("stdout:\n{}", stdout.trim_end());
    }

    if !stderr.trim().is_empty() {
        error!("stderr:\n{}", stderr.trim_end());
    }

    if !output.status.success() {
        return Err(anyhow!(
            "command exited with non-zero status: {}",
            output.status
        ));
    }

    Ok(())
}

#[cfg(windows)]
fn cmd_path(path: &Path) -> String {
    let s = path.to_string_lossy();

    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s.into_owned()
    }
}

#[cfg(windows)]
fn windows_shell_start_dir() -> OsString {
    std::env::var_os("SystemRoot").unwrap_or_else(|| OsString::from(r"C:\"))
}
