# Builds a signed installer and publishes it on the GitHub release "setup":
# the download link people get, and where the app's updater reads
# latest.json. Bump the version first (see the "Version x.y.z" commits).
#
#   powershell -ExecutionPolicy Bypass -File app\scripts\release.ps1 -Notes "What changed"
#
# Needs the updater signing key at %USERPROFILE%\.tauri\ownclips.key (its
# public half is in tauri.conf.json) and a logged-in gh.
param([string]$Notes = "")
$ErrorActionPreference = "Stop"
$app = Split-Path $PSScriptRoot -Parent
$root = Split-Path $app -Parent
$repo = "7Jerrybf/ownclips"
$tag = "setup"

$keyPath = Join-Path $env:USERPROFILE ".tauri\ownclips.key"
if (-not (Test-Path $keyPath)) { throw "Signing key not found: $keyPath" }
$version = (Get-Content (Join-Path $app "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
Write-Host "Releasing OwnClips $version"

$password = Read-Host "Signing key password" -AsSecureString
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content $keyPath -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD =
    [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($password))
# A running OwnClips locks its exe, which the build replaces; a running exe
# can still be renamed out of the way.
$built = Join-Path $root "target\release\ownclips-app.exe"
if (Test-Path $built) { Move-Item $built "$built.old" -Force -ErrorAction SilentlyContinue }
Push-Location $app
try {
    npm run tauri build
    if ($LASTEXITCODE -ne 0) { throw "Build failed" }
} finally {
    Pop-Location
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY, Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue
}

$bundle = Join-Path $root "target\release\bundle\nsis"
$name = "OwnClips_${version}_x64-setup.exe"
$exe = Join-Path $bundle $name
if (-not (Test-Path "$exe.sig")) { throw "Missing signature $exe.sig" }
$manifest = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = @{
        "windows-x86_64" = @{
            signature = (Get-Content "$exe.sig" -Raw).Trim()
            url       = "https://github.com/$repo/releases/download/$tag/$name"
        }
    }
}
$latest = Join-Path $bundle "latest.json"
# No BOM: the updater's JSON parser rejects it.
[IO.File]::WriteAllText($latest, ($manifest | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))

# The installer goes up before the manifest that points at it.
gh release upload $tag $exe "$exe.sig" --repo $repo --clobber
if ($LASTEXITCODE -ne 0) { throw "Upload failed" }
gh release upload $tag $latest --repo $repo --clobber
if ($LASTEXITCODE -ne 0) { throw "Upload failed" }
gh release edit $tag --repo $repo --title "v$version"

# Older installers would only confuse people picking a download.
$old = gh release view $tag --repo $repo --json assets --jq ".assets[].name" |
    Where-Object { $_ -like "OwnClips_*_x64-setup.exe*" -and $_ -notlike "$name*" }
foreach ($asset in $old) {
    gh release delete-asset $tag $asset --repo $repo --yes
}
Write-Host "Published $name to https://github.com/$repo/releases/tag/$tag"
