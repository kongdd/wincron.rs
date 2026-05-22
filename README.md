# wincron


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

## 初衷

> 让AI coding plan每天7点定时启动。（7-12、12-17、17-22），刚好符合下班时间。

使用方法

1. 新建一个cron.txt

```bash
# cron.txt
# 每天 7:00 开始，每 5 小时唤醒一次（7:00, 12:00, 17:00, 22:00）
0 0 7,12,17,22 * * * claude -p --model haiku --effort low "hi"

# 每天 7:00 开始，每 5 小时唤醒一次（7:00, 12:00, 17:00, 22:00）
0 0 7,12,17,22 * * * codex exec -m gpt-5.4-mini "hi"
```

2. wincron添加cron.txt

直接下载release中的wincron.exe文件。

> 不建议自己编译。
```bash
cargo install --git https://github.com/kongdd/wincron
cargo install --path .
```

```bash
wincron add cron.txt
wincron start         # 脱手管理，terminal可关，后台自动定时执行
```
