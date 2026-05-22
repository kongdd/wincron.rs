# wincron

A tiny Windows cron runner using local time.

## Features

- Cron-style task scheduling with 6-field format: `秒 分 时 日 月 星期`
- 5-field cron expressions are also accepted as `分 时 日 月 星期`
- Hot-reload `cron.txt` on changes
- Windows startup registration via registry
- Logging to `%LOCALAPPDATA%\WinCron\`
- Cross-platform task commands (R, Julia, PowerShell, etc.)

```bash
wincron -h
A tiny Windows cron runner written in Rust

Usage: wincron.exe <COMMAND>

Commands:
  run                Run cron scheduler in foreground (keeps window open)
  start              Start background daemon (reads all registered cron files)
  stop               Stop the running background daemon
  add                Add a cron file path to the daemon config
  delete             Remove a cron file path from the daemon config [aliases: rm]
  list               List registered cron file paths
  task               Show scheduled tasks from all registered cron files
  install-startup    Install current executable as Windows startup app
  uninstall-startup  Remove Windows startup registration
  status             Show cron tasks or startup registrations
  logs               Show or open log directory
  help               Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## Usage

```bash
# Run scheduler (default cron.txt)
wincron run

# Run with custom cron file and tick interval
wincron run --cron path/to/cron.txt --tick 5

# Install as Windows startup app
wincron install-startup

# Install startup with a specific cron file
wincron install-startup --cron C:\path\to\cron.txt

# Install startup with a custom registry value name
wincron install-startup --cron C:\path\to\cron.txt --name MyWinCron

# Remove startup registration
wincron uninstall-startup

# Remove a custom startup registration
wincron uninstall-startup --name MyWinCron

# Show task status
wincron status

# Show startup registration and its cron tasks
wincron status --startup

# Show or open log directory
wincron logs --open
```

## Windows Startup

`install-startup` writes the current executable to:

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
```

The registered command looks like:

```text
"C:\path\to\wincron.exe" run --cron "C:\path\to\cron.txt"
```

Use the release executable when registering startup:

```bash
cargo build --release
target\release\wincron.exe install-startup --cron C:\path\to\cron.txt
```

Check what is registered:

```bash
wincron status --startup
```

## cron.txt Format

```
# 秒 分 时 日 月 星期 命令

# Every 10 seconds
*/10 * * * * * echo hello >> wincron.log

# Every 10 seconds via PowerShell
*/10 * * * * * powershell -NoProfile -Command "Get-Date >> pwsh.log"

# Daily R script at 09:00
0 0 9 * * * Rscript.exe Scripts/hello.R >> rcron.log

# Daily Julia script at 23:30
0 30 23 * * * julia Scripts/hello.jl >> julia.log
```

## Log Output

Logs are written to `%LOCALAPPDATA%\WinCron\` with daily rotation.

## Build

```bash
cargo build --release
```
