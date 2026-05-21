pub mod runner;
pub mod startup;
pub mod tasks;

use std::path::PathBuf;

pub use tasks::{load_tasks, print_tasks, CronTask};

pub fn log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("WinCron")
}
