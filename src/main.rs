use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Local, Utc};
use clap::{Parser, Subcommand};
use cron::Schedule;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    thread,
    time::{Duration, SystemTime},
};
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "wincron")]
#[command(version)]
#[command(about = "A tiny Windows cron runner written in Rust")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run cron scheduler
    Run {
        /// Path to cron.txt
        #[arg(long, default_value = "cron.txt")]
        cron: PathBuf,

        /// Scheduler tick interval in seconds
        #[arg(long, default_value_t = 1)]
        tick: u64,
    },

    /// Install current executable as Windows startup app
    InstallStartup {
        /// Path to cron.txt
        #[arg(long, default_value = "cron.txt")]
        cron: PathBuf,

        /// Registry value name
        #[arg(long, default_value = "WinCron")]
        name: String,
    },

    /// Remove Windows startup registration
    UninstallStartup {
        /// Registry value name
        #[arg(long, default_value = "WinCron")]
        name: String,
    },

    /// Show cron tasks or startup registrations
    Status {
        /// Path to cron.txt
        #[arg(long, default_value = "cron.txt")]
        cron: PathBuf,

        /// Show registered wincron startup items
        #[arg(long)]
        startup: bool,
    },

    /// Show or open log directory
    Logs {
        /// Open log directory in Explorer
        #[arg(long)]
        open: bool,
    },
}

#[derive(Debug, Clone)]
struct CronTask {
    line_no: usize,
    expr: String,
    command: String,
    schedule: Schedule,
    next_run: Option<DateTime<Utc>>,
}

#[derive(Debug)]
struct StartupItem {
    name: String,
    command: String,
    cron_path: Option<PathBuf>,
}

fn main() -> Result<()> {
    let _log_guard = init_logging()?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Run { cron, tick } => run_scheduler(cron, tick),
        Commands::InstallStartup { cron, name } => install_startup(&name, &cron),
        Commands::UninstallStartup { name } => uninstall_startup(&name),
        Commands::Status { cron, startup } => show_status(cron, startup),
        Commands::Logs { open } => show_logs(open),
    }
}

fn run_scheduler(cron_path: PathBuf, tick_seconds: u64) -> Result<()> {
    let cron_path = fs::canonicalize(&cron_path)
        .with_context(|| format!("cannot find cron file: {}", cron_path.display()))?;

    info!("wincron started");
    info!("cron file: {}", cron_path.display());
    info!("log dir: {}", log_dir().display());

    let task_dir = cron_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let mut last_modified: Option<SystemTime> = None;
    let mut tasks: Vec<CronTask> = Vec::new();

    loop {
        let modified = fs::metadata(&cron_path)
            .and_then(|m| m.modified())
            .ok();

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

            if let Some(next) = task.next_run {
                if now >= next {
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
            }
        }

        thread::sleep(Duration::from_secs(tick_seconds.max(1)));
    }
}

fn show_status(cron_path: PathBuf, startup: bool) -> Result<()> {
    if startup {
        let items = list_startup_items()?;

        if items.is_empty() {
            println!("No wincron startup item found.");
            return Ok(());
        }

        println!("Startup wincron items:");
        println!();

        for item in items {
            println!("Name    : {}", item.name);
            println!("Command : {}", item.command);

            if let Some(cron) = item.cron_path {
                println!("Cron    : {}", cron.display());

                match load_tasks(&cron) {
                    Ok(tasks) => print_tasks(&tasks),
                    Err(e) => println!("Failed to read cron file: {e:?}"),
                }
            } else {
                println!("Cron    : <not found in command>");
            }

            println!();
        }

        return Ok(());
    }

    let cron_path = fs::canonicalize(&cron_path)
        .with_context(|| format!("cannot find cron file: {}", cron_path.display()))?;

    println!("Cron file: {}", cron_path.display());
    println!();

    let tasks = load_tasks(&cron_path)?;
    print_tasks(&tasks);

    Ok(())
}

fn print_tasks(tasks: &[CronTask]) {
    if tasks.is_empty() {
        println!("No cron task found.");
        return;
    }

    println!(
        "{:<6} {:<24} {:<24} {}",
        "Line", "Next Run Local", "Cron Expr", "Command"
    );
    println!("{}", "-".repeat(110));

    for task in tasks {
        let next_local = task
            .next_run
            .map(|t| {
                t.with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|| "<none>".to_string());

        println!(
            "{:<6} {:<24} {:<24} {}",
            task.line_no, next_local, task.expr, task.command
        );
    }
}

fn load_tasks(path: &Path) -> Result<Vec<CronTask>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let mut tasks = Vec::new();

    for (idx, raw_line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match parse_cron_line(line) {
            Ok((expr, command, schedule)) => {
                let next_run = schedule.upcoming(Utc).next();

                info!(
                    "line {} next run {:?}: {}",
                    line_no, next_run, command
                );

                tasks.push(CronTask {
                    line_no,
                    expr,
                    command,
                    schedule,
                    next_run,
                });
            }
            Err(e) => {
                warn!("skip invalid line {}: {}", line_no, e);
            }
        }
    }

    Ok(tasks)
}

/// 支持：
///
/// 6 字段：
///   sec min hour day month weekday command...
///
/// 5 字段：
///   min hour day month weekday command...
///
/// 推荐统一使用 6 字段。
fn parse_cron_line(line: &str) -> Result<(String, String, Schedule)> {
    let spans = token_spans(line);

    if spans.len() < 6 {
        return Err(anyhow!("too few fields"));
    }

    // 先尝试 6 字段 cron：
    // sec min hour day month weekday command...
    if spans.len() >= 7 {
        let expr = line[spans[0].0..spans[5].1].to_string();
        let command = line[spans[6].0..].trim().to_string();

        if let Ok(schedule) = Schedule::from_str(&expr) {
            return Ok((expr, command, schedule));
        }
    }

    // 再尝试 5 字段 cron：
    // min hour day month weekday command...
    // 自动补 seconds = 0
    if spans.len() >= 6 {
        let expr_5 = line[spans[0].0..spans[4].1].to_string();
        let expr_6 = format!("0 {}", expr_5);
        let command = line[spans[5].0..].trim().to_string();

        let schedule = Schedule::from_str(&expr_6)
            .with_context(|| format!("invalid cron expression: {expr_5}"))?;

        return Ok((expr_6, command, schedule));
    }

    Err(anyhow!("invalid cron line"))
}

/// 返回每个非空白 token 在原始字符串中的 byte span。
///
/// 注意：
/// 这里仅用于切分 cron 字段，命令部分会原样保留。
fn token_spans(s: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;

    for (idx, ch) in s.char_indices() {
        if ch.is_whitespace() {
            if let Some(st) = start.take() {
                spans.push((st, idx));
            }
        } else if start.is_none() {
            start = Some(idx);
        }
    }

    if let Some(st) = start {
        spans.push((st, s.len()));
    }

    spans
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

    let finished_at = Local::now();

    info!(
        "task finished at {}",
        finished_at.format("%Y-%m-%d %H:%M:%S")
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

fn log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("WinCron")
}

fn init_logging() -> Result<tracing_appender::non_blocking::WorkerGuard> {
    use tracing::Level;
    use tracing_subscriber::fmt::writer::MakeWriterExt;

    let dir = log_dir();

    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create log dir: {}", dir.display()))?;

    let file_appender = tracing_appender::rolling::daily(&dir, "wincron.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let stdout = std::io::stdout.with_max_level(Level::INFO);

    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .with_writer(non_blocking.and(stdout))
        .init();

    info!("logging initialized");
    info!("log dir: {}", dir.display());

    Ok(guard)
}

fn show_logs(open: bool) -> Result<()> {
    let dir = log_dir();

    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create log dir: {}", dir.display()))?;

    println!("{}", dir.display());

    if open {
        #[cfg(windows)]
        {
            Command::new("explorer")
                .arg(&dir)
                .spawn()
                .context("failed to open log directory")?;
        }

        #[cfg(not(windows))]
        {
            println!("open is only implemented for Windows");
        }
    }

    Ok(())
}

#[cfg(windows)]
fn install_startup(name: &str, cron_path: &Path) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let exe = std::env::current_exe()
        .context("failed to get current exe path")?;

    let cron_abs = fs::canonicalize(cron_path)
        .unwrap_or_else(|_| cron_path.to_path_buf());

    let command = format!(
        "\"{}\" run --cron \"{}\"",
        exe.display(),
        cron_abs.display()
    );

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key_path = r"Software\Microsoft\Windows\CurrentVersion\Run";

    let (run_key, _) = hkcu
        .create_subkey(run_key_path)
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
fn install_startup(_name: &str, _cron_path: &Path) -> Result<()> {
    Err(anyhow!("install-startup is only supported on Windows"))
}

#[cfg(windows)]
fn uninstall_startup(name: &str) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key_path = r"Software\Microsoft\Windows\CurrentVersion\Run";

    let run_key = hkcu
        .open_subkey_with_flags(run_key_path, KEY_SET_VALUE)
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
fn uninstall_startup(_name: &str) -> Result<()> {
    Err(anyhow!("uninstall-startup is only supported on Windows"))
}

#[cfg(windows)]
fn list_startup_items() -> Result<Vec<StartupItem>> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key_path = r"Software\Microsoft\Windows\CurrentVersion\Run";

    let run_key = hkcu
        .open_subkey_with_flags(run_key_path, KEY_READ)
        .context("failed to open registry Run key")?;

    let mut items = Vec::new();

    for value in run_key.enum_values() {
        let (name, _) = value?;

        let command: String = match run_key.get_value(&name) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let command_lc = command.to_lowercase();

        if command_lc.contains("wincron") && command_lc.contains(" run ") {
            let cron_path = extract_cron_path(&command);

            items.push(StartupItem {
                name,
                command,
                cron_path,
            });
        }
    }

    Ok(items)
}

#[cfg(not(windows))]
fn list_startup_items() -> Result<Vec<StartupItem>> {
    Ok(Vec::new())
}

/// 从启动命令中提取 --cron 后面的路径。
///
/// 支持：
///   wincron run --cron C:\a\cron.txt
///   wincron run --cron "C:\a b\cron.txt"
///   wincron run --cron=C:\a\cron.txt
fn extract_cron_path(command: &str) -> Option<PathBuf> {
    let args = split_command_like_windows(command);

    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        if arg == "--cron" {
            return iter.next().map(PathBuf::from);
        }

        if let Some(rest) = arg.strip_prefix("--cron=") {
            return Some(PathBuf::from(rest));
        }
    }

    None
}

/// 一个够用的 Windows 风格命令行切分器。
///
/// 支持：
/// - 空格切分；
/// - 双引号中的空格保留；
/// - 去掉外层双引号。
///
/// 不追求完全复刻 cmd.exe 的所有转义规则。
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
