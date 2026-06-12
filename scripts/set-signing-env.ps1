# Set Tauri signing env vars (run in PowerShell before build)
# Usage: . .\scripts\set-signing-env.ps1 (note the dot+space prefix)

$privateKey = Get-Content .\tauri.key -Raw
$env:TAURI_SIGNING_PRIVATE_KEY = $privateKey
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "123456"

Write-Host "OK - Signing env vars set. Ready for: pnpm tauri build" -ForegroundColor Green
