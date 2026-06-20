Get-Process | Where-Object { $_.ProcessName -like "*meowmic*" } | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2
$exePath = "D:\web\pico-denoise\src-tauri\target\debug\meowmic.exe"
if (Test-Path $exePath) {
    Remove-Item -Force $exePath -ErrorAction SilentlyContinue
    if (Test-Path $exePath) {
        Write-Host "STILL LOCKED - trying handle"
        # Find who locks it
        Get-Process | ForEach-Object {
            if ($_.Path -and $_.Path -ne "") {
                Write-Host "$($_.Id) $($_.ProcessName) $($_.Path)"
            }
        }
    } else {
        Write-Host "DELETED OK"
    }
} else {
    Write-Host "FILE GONE"
}
