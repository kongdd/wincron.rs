$log = Join-Path (Split-Path -Parent $PSScriptRoot) "ps1.log"
$now = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
Add-Content -Path $log -Value "hello from ps1 at $now"
