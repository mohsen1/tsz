# tsz installer — Windows / PowerShell
# Usage:
#   irm https://tsz.dev/install.ps1 | iex
#   & ([ScriptBlock]::Create((irm https://tsz.dev/install.ps1))) -Version v0.1.9 -InstallDir $HOME\bin
[CmdletBinding()]
param(
    [string]$Version = $(if ($env:TSZ_VERSION) { $env:TSZ_VERSION } else { "latest" }),
    [string]$InstallDir = $(if ($env:TSZ_INSTALL_DIR) { $env:TSZ_INSTALL_DIR } else { "$HOME\.local\bin" }),
    [string]$Owner = $(if ($env:TSZ_REPO_OWNER) { $env:TSZ_REPO_OWNER } else { "mohsen1" }),
    [string]$Repo = $(if ($env:TSZ_REPO_NAME) { $env:TSZ_REPO_NAME } else { "tsz" })
)

$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

function Say($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Warn($msg) { Write-Host "!! $msg" -ForegroundColor Yellow }
function Die($msg) { Write-Host "xx $msg" -ForegroundColor Red; exit 1 }

if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

$arch = $env:PROCESSOR_ARCHITECTURE
switch ($arch) {
    "AMD64" { $target = "x86_64-pc-windows-msvc" }
    "ARM64" { $target = "aarch64-pc-windows-msvc" }
    default { Die "unsupported Windows arch: $arch" }
}

if ($Version -eq "latest") {
    try {
        $rel = Invoke-RestMethod "https://api.github.com/repos/$Owner/$Repo/releases/latest"
        $Version = $rel.tag_name
    } catch {
        Die "failed to fetch latest release tag from GitHub"
    }
}

$asset = "tsz-$Version-$target.zip"
$url = "https://github.com/$Owner/$Repo/releases/download/$Version/$asset"

Say "version:     $Version"
Say "target:      $target"
Say "asset:       $asset"
Say "install dir: $InstallDir"
Say "url:         $url"

$tmp = New-Item -ItemType Directory -Force -Path (Join-Path $env:TEMP "tsz-install-$([guid]::NewGuid().Guid)")
try {
    $zipPath = Join-Path $tmp $asset
    Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing

    Say "extracting"
    Expand-Archive -Path $zipPath -DestinationPath $tmp -Force

    $inner = Join-Path $tmp "tsz-$Version-$target"
    if (-not (Test-Path $inner)) {
        $inner = Join-Path $tmp "tsz-$target"
    }
    if (-not (Test-Path $inner)) {
        Die "unexpected tarball layout"
    }

    foreach ($bin in @("tsz.exe", "tsz-lsp.exe")) {
        $src = Join-Path $inner $bin
        if (Test-Path $src) {
            Copy-Item -Force $src (Join-Path $InstallDir $bin)
            Say "installed $InstallDir\$bin"
        }
    }
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

$pathDirs = $env:PATH -split ";"
$resolved = (Resolve-Path $InstallDir).Path
if ($pathDirs -notcontains $resolved) {
    Warn "$InstallDir is not on your PATH"
    Warn "add it via:  [Environment]::SetEnvironmentVariable('PATH', `"$resolved;`$env:PATH`", 'User')"
}

Say "done — try: tsz --version"
