# Puts ffmpeg.exe (and its license) into src-tauri/resources/ffmpeg so the
# installer ships it. Copies the build winget installs (Gyan.FFmpeg),
# installing it first if needed. ffmpeg is too large to keep in git.
$ErrorActionPreference = "Stop"
$dest = Join-Path $PSScriptRoot "..\src-tauri\resources\ffmpeg"
if (Test-Path (Join-Path $dest "ffmpeg.exe")) {
    Write-Host "ffmpeg already prepared in $dest"
    exit 0
}

function Find-Gyan {
    Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\Gyan.FFmpeg*\*\bin\ffmpeg.exe" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
}

$exe = Find-Gyan
if (-not $exe) {
    Write-Host "Installing ffmpeg with winget (Gyan.FFmpeg)..."
    winget install --id Gyan.FFmpeg -e --accept-source-agreements --accept-package-agreements --disable-interactivity
    $exe = Find-Gyan
    if (-not $exe) { throw "ffmpeg not found after installing Gyan.FFmpeg" }
}

New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item $exe.FullName (Join-Path $dest "ffmpeg.exe")
Copy-Item (Join-Path $exe.Directory.Parent.FullName "LICENSE") (Join-Path $dest "LICENSE")
Write-Host "Copied $($exe.FullName) -> $dest"
