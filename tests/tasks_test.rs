use std::fs;
use tempfile::TempDir;

fn temp_cron_file(content: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let cron_path = dir.path().join("cron.txt");
    fs::write(&cron_path, content).unwrap();
    (dir, cron_path)
}

#[test]
fn test_load_empty_file() {
    let (_dir, path) = temp_cron_file("");
    let tasks = wincron::tasks::load_tasks(&path).unwrap();
    assert!(tasks.is_empty());
}

#[test]
fn test_load_comments_only() {
    let (_dir, path) = temp_cron_file("# comment\n# another comment\n");
    let tasks = wincron::tasks::load_tasks(&path).unwrap();
    assert!(tasks.is_empty());
}

#[test]
fn test_load_single_task() {
    let (_dir, path) = temp_cron_file("*/10 * * * * * echo hello");
    let tasks = wincron::tasks::load_tasks(&path).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].line_no, 1);
    assert_eq!(tasks[0].command, "echo hello");
    assert!(tasks[0].next_run.is_some());
}

#[test]
fn test_load_multiple_tasks() {
    let content = "*/10 * * * * * echo hello\n*/5 * * * * * echo world\n";
    let (_dir, path) = temp_cron_file(content);
    let tasks = wincron::tasks::load_tasks(&path).unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].command, "echo hello");
    assert_eq!(tasks[1].command, "echo world");
}

#[test]
fn test_load_mixed_lines() {
    let content = "# comment line\n\n   \n*/10 * * * * * echo hello\n";
    let (_dir, path) = temp_cron_file(content);
    let tasks = wincron::tasks::load_tasks(&path).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].line_no, 4);
}

#[test]
fn test_load_5_field_cron() {
    let (_dir, path) = temp_cron_file("0 9 * * * Rscript.exe script.R");
    let tasks = wincron::tasks::load_tasks(&path).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].expr, "0 0 9 * * *");
    assert_eq!(tasks[0].command, "Rscript.exe script.R");
}

#[test]
fn test_load_task_next_run_is_set() {
    let (_dir, path) = temp_cron_file("*/10 * * * * * echo test");
    let tasks = wincron::tasks::load_tasks(&path).unwrap();
    assert!(tasks[0].next_run.is_some());
}

#[test]
fn test_load_nonexistent_file() {
    let result = wincron::tasks::load_tasks(std::path::Path::new("nonexistent.txt"));
    assert!(result.is_err());
}

#[test]
fn test_load_invalid_cron_expression() {
    let (_dir, path) = temp_cron_file("a b c d e f g");
    let tasks = wincron::tasks::load_tasks(&path).unwrap();
    // Invalid lines are skipped, but parsing continues
    // The parse_cron_line will fail, but load_tasks continues
    assert!(tasks.is_empty());
}

#[test]
fn test_task_with_special_characters_in_command() {
    let (_dir, path) = temp_cron_file("0 9 * * * * powershell -NoProfile -Command \"Get-Date\"");
    let tasks = wincron::tasks::load_tasks(&path).unwrap();
    assert_eq!(tasks.len(), 1);
    assert!(tasks[0].command.contains("Get-Date"));
}
