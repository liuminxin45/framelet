param([string]$Destination = (Join-Path $PSScriptRoot '..\dist'))
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $root 'target\release\framelet.exe') -Destination $Destination
    Copy-Item -LiteralPath (Join-Path $root 'README.md'), (Join-Path $root 'README.zh-CN.md'), (Join-Path $root 'LICENSE'), (Join-Path $root 'CHANGELOG.md') -Destination $Destination
    Copy-Item -LiteralPath (Join-Path $root 'docs') -Destination $Destination -Recurse -Force
    python scripts/licenses.py $Destination
    if ($LASTEXITCODE -ne 0) { throw 'Dependency license collection failed' }
    $binary = Join-Path $Destination 'framelet.exe'
    (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLower() + '  framelet.exe' | Set-Content -Encoding ascii (Join-Path $Destination 'SHA256SUMS.txt')
    $payload = @('framelet.exe','README.md','README.zh-CN.md','LICENSE','CHANGELOG.md','SHA256SUMS.txt','dependencies.json','THIRD_PARTY_NOTICES.md','licenses','docs') | ForEach-Object { Join-Path $Destination $_ }
    $metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object name -eq 'framelet').version
    Compress-Archive -LiteralPath $payload -DestinationPath (Join-Path $Destination ('framelet-v'+$version+'-windows-x64.zip')) -Force
} finally { Pop-Location }
