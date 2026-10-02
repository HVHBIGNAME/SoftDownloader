param([string]$OutputDirectory = "")
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path -LiteralPath $root)) { throw 'Project root was not found' }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $root 'dist' }
$manifest = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
$version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $version) { throw 'Package version was not found' }
$name = "SoftDownloader-$version-windows-x64"
$stage = Join-Path $OutputDirectory $name
foreach ($binary in @('softdownloader.exe', 'catalog-check.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $root "target\release\$binary"))) { throw "Build first: cargo build --release --bins --locked ($binary missing)" }
}
[void](New-Item -ItemType Directory -Path $stage -Force)
Copy-Item -LiteralPath (Join-Path $root 'target\release\softdownloader.exe') -Destination (Join-Path $stage 'SoftDownloader.exe') -Force
Copy-Item -LiteralPath (Join-Path $root 'target\release\catalog-check.exe') -Destination $stage -Force
foreach ($file in @('README.md', 'LICENSE')) { Copy-Item -LiteralPath (Join-Path $root $file) -Destination $stage -Force }
foreach ($directory in @('docs', 'catalog')) { Copy-Item -LiteralPath (Join-Path $root $directory) -Destination $stage -Recurse -Force }
[void](New-Item -ItemType Directory -Path (Join-Path $stage 'tools') -Force)
Copy-Item -LiteralPath (Join-Path $root 'tools\catalog.py') -Destination (Join-Path $stage 'tools') -Force
$zip = Join-Path $OutputDirectory "$name.zip"
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -Force
$digest = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath "$zip.sha256" -Value "$digest  $name.zip" -Encoding Ascii
"Package: $zip"
"SHA-256: $digest"
