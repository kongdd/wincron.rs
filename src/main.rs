mod config;
mod runner;
mod startup;
mod tasks;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};
use tasks::{load_tasks, print_tasks};
use tracing::info;

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
    /// Run cron scheduler in foreground (keeps window open)
    Run {
        /// Path to cron.txt
        #[arg(long, default_value = "cron.txt")]
        cron: PathBuf,

        /// Scheduler tick interval in seconds
        #[arg(long, default_value_t = 1)]
        tick: u64,
    },

    /// Start background daemon (reads all registered cron files)
    #[command(alias = "daemon")]
    Start,

    /// Stop the running background daemon
    Stop,

    /// Add a cron file path to the daemon config
    #[command(alias = "a")]
    Add {
        /// Path to cron.txt
        path: PathBuf,
    },

    /// Remove a cron file path from the daemon config
    #[command(alias = "d", visible_alias = "rm")]
    Delete {
        /// Path to cron.txt
        path: PathBuf,
    },

    /// List registered cron file paths
    List,

    /// Show scheduled tasks from all registered cron files
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },

    /// Install current executable as Windows startup app
    InstallStartup {
        /// Path to cron.txt (omit to use daemon mode with registered files)
        #[arg(long)]
        cron: Option<PathBuf>,

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

    #[command(hide = true)]
    DaemonRun,
}

#[derive(Subcommand, Debug)]
enum TaskAction {
    /// List all scheduled tasks from registered cron files
    List,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let _log_guard = init_logging()?;

    match cli.command {
        Commands::Run { cron, tick } => runner::run_scheduler(cron, tick),
        Commands::Start => start_daemon(),
        Commands::Stop => stop_daemon(),
        Commands::DaemonRun => runner::run_daemon(1),
        Commands::Add { path } => cmd_add(path),
        Commands::Delete { path } => cmd_delete(path),
        Commands::List => cmd_list(),
        Commands::Task { action } => match action {
            TaskAction::List => cmd_task_list(),
        },
        Commands::InstallStartup { cron, name } => {
            startup::install_startup(&name, cron.as_deref())
        }
        Commands::UninstallStartup { name } => startup::uninstall_startup(&name),
        Commands::Status { cron, startup } => show_status(cron, startup),
        Commands::Logs { open } => show_logs(open),
    }
}

pub fn log_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")))
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

fn start_daemon() -> Result<()> {
    let exe = std::env::current_exe().context("failed to get executable path")?;

    let paths = config::load_paths()?;
    if paths.is_empty() {
        println!("No cron files registered.");
        println!("Use 'wincron add <path>' to register a cron file first.");
        return Ok(());
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;

        Command::new(&exe)
            .arg("daemon-run")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .context("failed to start daemon")?;
    }

    #[cfg(not(windows))]
    {
        Command::new(&exe)
            .arg("daemon-run")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to start daemon")?;
    }

    // Brief pause so daemon-run has time to write its PID file
    std::thread::sleep(std::time::Duration::from_millis(200));

    let pid_info = config::read_pid()
        .map(|p| format!("PID {}", p))
        .unwrap_or_else(|| "PID unknown".to_string());

    println!("Daemon started in background ({}, {} cron file(s)).", pid_info, paths.len());
    println!("Config : {}", config::config_path().display());
    println!("Logs   : {}", log_dir().display());
    println!("Stop   : wincron stop");
    Ok(())
}

fn stop_daemon() -> Result<()> {
    let Some(pid) = config::read_pid() else {
        println!("Daemon is not running (no PID file found).");
        println!("PID file: {}", config::pid_path().display());
        return Ok(());
    };

    #[cfg(windows)]
    {
        let output = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output()
            .context("failed to run taskkill")?;

        if output.status.success() {
            println!("Daemon stopped (PID {}).", pid);
            config::remove_pid();
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // taskkill outputs "ERROR: The process ... not found" when already gone
            if output.stderr.is_empty() || stderr.to_lowercase().contains("not found") {
                println!("Process {} not found, removing stale PID file.", pid);
                config::remove_pid();
            } else {
                println!("Failed to stop daemon: {}", stderr.trim());
            }
        }
    }

    #[cfg(not(windows))]
    {
        let output = Command::new("kill")
            .args([&pid.to_string()])
            .output()
            .context("failed to run kill")?;

        if output.status.success() {
            println!("Daemon stopped (PID {}).", pid);
        } else {
            println!("Process {} may already be stopped.", pid);
        }
        config::remove_pid();
    }

    Ok(())
}

fn cmd_add(path: PathBuf) -> Result<()> {
    let canonical = config::add_path(&path)?;
    println!("Added  : {}", canonical.display());
    println!("Config : {}", config::config_path().display());
    Ok(())
}

fn cmd_delete(path: PathBuf) -> Result<()> {
    if config::delete_path(&path)? {
        println!("Removed: {}", path.display());
    } else {
        println!("Not found in config: {}", path.display());
    }
    Ok(())
}

fn cmd_list() -> Result<()> {
    let paths = config::load_paths()?;
    if paths.is_empty() {
        println!("No cron files registered.");
        println!("Use 'wincron add <path>' to add one.");
        return Ok(());
    }
    println!("Registered cron files ({}):", paths.len());
    for (i, p) in paths.iter().enumerate() {
        let tag = if p.exists() { "" } else { "  [NOT FOUND]" };
        println!("  {}. {}{}", i + 1, p.display(), tag);
    }
    println!();
    println!("Config : {}", config::config_path().display());
    Ok(())
}

fn cmd_task_list() -> Result<()> {
    let paths = config::load_paths()?;
    if paths.is_empty() {
        println!("No cron files registered. Use 'wincron add <path>' first.");
        return Ok(());
    }
    for path in &paths {
        println!("=== {} ===", path.display());
        match load_tasks(path) {
            Ok(tasks) => print_tasks(&tasks),
            Err(e) => println!("  Error: {e}"),
        }
        println!();
    }
    Ok(())
}

fn show_logs(open: bool) -> Result<()> {
    let dir = log_dir();

    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create log dir: {}", dir.display()))?;

    println!("{}", dir.display());

    if open {
        #[cfg(windows)]
        Command::new("explorer")
            .arg(&dir)
            .spawn()
            .context("failed to open log directory")?;

        #[cfg(not(windows))]
        println!("open is only implemented for Windows");
    }

    Ok(())
}

fn show_status(cron_path: PathBuf, startup: bool) -> Result<()> {
    if startup {
        return show_startup_status();
    }

    let cron_path = fs::canonicalize(&cron_path)
        .with_context(|| format!("cannot find cron file: {}", cron_path.display()))?;

    println!("Cron file: {}", cron_path.display());
    println!();

    print_tasks(&load_tasks(&cron_path)?);
    Ok(())
}

fn show_startup_status() -> Result<()> {
    let items = startup::list_startup_items()?;

    if items.is_empty() {
        println!("No wincron startup item found.");
        return Ok(());
    }

    println!("Startup wincron items:");
    println!();

    for item in items {
        println!("Name    : {}", item.name);
        println!("Command : {}", item.command);
        print_startup_cron(item.cron_path);
        println!();
    }

    Ok(())
}

fn print_startup_cron(cron_path: Option<PathBuf>) {
    let Some(cron) = cron_path else {
        println!("Cron    : <not found in command>");
        return;
    };

    println!("Cron    : {}", cron.display());
    match load_tasks(&cron) {
        Ok(tasks) => print_tasks(&tasks),
        Err(e) => println!("Failed to read cron file: {e:?}"),
    }
}
