param([string]$Destination = (Join-Path $PSScriptRoot '..\dist'))
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    $Destination = (Get-Item -LiteralPath $Destination).FullName
    # Fresh staging prevents old media or old notices leaking into a new archive.
    $packageStage = Join-Path $Destination ('portable-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $packageStage | Out-Null
    $metadataText = cargo metadata --no-deps --locked --format-version 1
    if ($LASTEXITCODE -ne 0) { throw "Cargo metadata failed" }
    $metadata = $metadataText | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object name -eq 'framelet').version
    if ([string]::IsNullOrWhiteSpace($version)) { throw "Missing package version" }
    Copy-Item -LiteralPath (Join-Path $root 'target\release\framelet.exe') -Destination $packageStage
    Copy-Item -LiteralPath (Join-Path $root 'README.md'), (Join-Path $root 'README.zh-CN.md'), (Join-Path $root 'ABOUT.md'), (Join-Path $root 'AGENTS.md'), (Join-Path $root 'LICENSE'), (Join-Path $root 'CHANGELOG.md') -Destination $packageStage
    Copy-Item -LiteralPath (Join-Path $root 'skills') -Destination $packageStage -Recurse
    # Recorder + text docs + notices. Demo GIFs/PNGs are separate release assets.
    $docsSource = Join-Path $root 'docs'
    Get-ChildItem -LiteralPath $docsSource -File -Recurse | Where-Object { $_.Extension -in @('.md','.json') } | ForEach-Object {
        $relativeDoc = $_.FullName.Substring($docsSource.Length + 1)
        $docTarget = Join-Path (Join-Path $packageStage 'docs') $relativeDoc
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $docTarget) | Out-Null
        Copy-Item -LiteralPath $_.FullName -Destination $docTarget
    }
    Get-ChildItem -LiteralPath $packageStage -Filter '*.md' -File -Recurse | ForEach-Object {
        $content = Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8
        $mediaRef = if ($version.EndsWith("-dev")) { "main" } else { "v$version" }
        $mediaBase = "https://raw.githubusercontent.com/liuminxin45/framelet/$mediaRef/docs/images/"
        $content.Replace('(docs/images/', '(' + $mediaBase).Replace('(images/', '(' + $mediaBase) | Set-Content -LiteralPath $_.FullName -Encoding UTF8 -NoNewline
    }
    python scripts/licenses.py $packageStage
    if ($LASTEXITCODE -ne 0) { throw 'Dependency license collection failed' }
    $binary = Join-Path $packageStage 'framelet.exe'
    (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLower() + '  framelet.exe' | Set-Content -Encoding ascii (Join-Path $packageStage 'SHA256SUMS.txt')
    $payload = @('framelet.exe','README.md','README.zh-CN.md','ABOUT.md','AGENTS.md','LICENSE','CHANGELOG.md','SHA256SUMS.txt','dependencies.json','THIRD_PARTY_NOTICES.md','licenses','docs','skills') | ForEach-Object { Join-Path $packageStage $_ }
    $archive = Join-Path $Destination ('framelet-v'+$version+'-windows-x64.zip')
    $archiveStage = Join-Path $Destination ('archive-' + [guid]::NewGuid().ToString('N') + '.zip')
    try {
        Compress-Archive -LiteralPath $payload -DestinationPath $archiveStage
        Move-Item -LiteralPath $archiveStage -Destination $archive -Force
    } catch {
        # Retain staging evidence and report the failed step; do not claim success.
        throw "Package publication failed (staging: $archiveStage): $_"
    }
    [pscustomobject]@{archive=$archive;exe_bytes=(Get-Item -LiteralPath $binary).Length;zip_bytes=(Get-Item -LiteralPath $archive).Length;sha256=(Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLower()} | ConvertTo-Json -Compress
    python scripts/build_distribution.py $packageStage $Destination
    if ($LASTEXITCODE -ne 0) { throw 'Client distribution packaging failed' }
} finally { Pop-Location }
