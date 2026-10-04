param([string]$OutputDirectory = "")
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path -LiteralPath $root)) { throw 'Project root was not found' }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $root 'dist' }
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
if (-not (Test-Path -LiteralPath (Split-Path -Parent $OutputDirectory))) { throw 'Output parent directory does not exist' }
$manifest = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
$version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $version) { throw 'Package version was not found' }
$binary = Join-Path $root 'target\release\softdownloader.exe'
if (-not (Test-Path -LiteralPath $binary)) { throw 'Build first: cargo build --release --locked' }
[void](New-Item -ItemType Directory -Path $OutputDirectory -Force)
$destination = Join-Path $OutputDirectory 'SoftDownloader.exe'
Copy-Item -LiteralPath $binary -Destination $destination -Force
$digest = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath "$destination.sha256" -Value "$digest  SoftDownloader.exe" -Encoding Ascii
$bytes = (Get-Item -LiteralPath $destination).Length
"Standalone $version`: $destination"
"Size: $bytes bytes ($([math]::Round($bytes / 1MB, 2)) MiB)"
"SHA-256: $digest"
