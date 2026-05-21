# wincron

A tiny Windows cron runner written in Rust.

## Features

- Cron-style task scheduling with 6-field format: `秒 分 时 日 月 星期`
- Hot-reload `cron.txt` on changes
- Windows startup registration via registry
- Logging to `%LOCALAPPDATA%\WinCron\`
- Cross-platform task commands (R, Julia, PowerShell, etc.)

## Usage

```bash
# Run scheduler (default cron.txt)
cargo run -- run

# Run with custom cron file and tick interval
cargo run -- run --cron path/to/cron.txt --tick 5

# Install as Windows startup app
cargo run -- install-startup

# Remove startup registration
cargo run -- uninstall-startup

# Show task status
cargo run -- status

# Show or open log directory
cargo run -- logs --open
```

## cron.txt Format

```
# 秒 分 时 日 月 星期 命令

# Every 10 seconds
*/10 * * * * * echo hello >> wincron.log

# Every 5 minutes via PowerShell
*/10 * * * * * powershell -NoProfile -Command "Get-Date >> pwsh.log"

# Daily R script at 09:00
0 9 * * * * Rscript.exe Scripts/hello.R >> rcron.log

# Daily Julia script at 23:30
0 23 * * * * julia Scripts/hello.jl >> julia.log
```

## Log Output

Logs are written to `%LOCALAPPDATA%\WinCron\` with daily rotation.

## Build

```bash
cargo build --release
```
