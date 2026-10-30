param([string]$Destination = (Join-Path $PSScriptRoot '..\dist'))
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $root 'target\release\framelet.exe') -Destination $Destination
    Copy-Item -LiteralPath (Join-Path $root 'README.md'), (Join-Path $root 'LICENSE') -Destination $Destination
    python scripts/licenses.py $Destination
    if ($LASTEXITCODE -ne 0) { throw 'Dependency license collection failed' }
    $binary = Join-Path $Destination 'framelet.exe'
    (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLower() + '  framelet.exe' | Set-Content -Encoding ascii (Join-Path $Destination 'SHA256SUMS.txt')
    Compress-Archive -LiteralPath $binary,(Join-Path $Destination 'README.md'),(Join-Path $Destination 'LICENSE'),(Join-Path $Destination 'SHA256SUMS.txt'),(Join-Path $Destination 'dependencies.json'),(Join-Path $Destination 'THIRD_PARTY_NOTICES.md'),(Join-Path $Destination 'licenses') -DestinationPath (Join-Path $Destination 'framelet-v0.1.0-windows-x64.zip') -Force
} finally { Pop-Location }
