mod runner;
mod startup;
mod tasks;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::{fs, path::PathBuf, process::Command};
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

fn main() -> Result<()> {
    let _log_guard = init_logging()?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Run { cron, tick } => runner::run_scheduler(cron, tick),
        Commands::InstallStartup { cron, name } => startup::install_startup(&name, &cron),
        Commands::UninstallStartup { name } => startup::uninstall_startup(&name),
        Commands::Status { cron, startup } => show_status(cron, startup),
        Commands::Logs { open } => show_logs(open),
    }
}

pub fn log_dir() -> PathBuf {
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

        match item.cron_path {
            Some(cron) => {
                println!("Cron    : {}", cron.display());
                match load_tasks(&cron) {
                    Ok(tasks) => print_tasks(&tasks),
                    Err(e) => println!("Failed to read cron file: {e:?}"),
                }
            }
            None => println!("Cron    : <not found in command>"),
        }

        println!();
    }

    Ok(())
}
