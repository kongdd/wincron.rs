use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Local, Utc};
use cron::Schedule;
use std::{fs, path::Path, str::FromStr};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct CronTask {
    pub line_no: usize,
    pub expr: String,
    pub command: String,
    pub schedule: Schedule,
    pub next_run: Option<DateTime<Utc>>,
}

pub fn load_tasks(path: &Path) -> Result<Vec<CronTask>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

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

                info!("line {} next run {:?}: {}", line_no, next_run, command);

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

pub fn print_tasks(tasks: &[CronTask]) {
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

/// Supports both 6-field cron lines:
///   sec min hour day month weekday command...
///
/// and 5-field cron lines:
///   min hour day month weekday command...
///
/// Five-field expressions are normalized by prepending seconds = 0.
fn parse_cron_line(line: &str) -> Result<(String, String, Schedule)> {
    let spans = token_spans(line);

    if spans.len() < 6 {
        return Err(anyhow!("too few fields"));
    }

    if spans.len() >= 7 {
        let expr = line[spans[0].0..spans[5].1].to_string();
        let command = line[spans[6].0..].trim().to_string();

        if let Ok(schedule) = Schedule::from_str(&expr) {
            return Ok((expr, command, schedule));
        }
    }

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
