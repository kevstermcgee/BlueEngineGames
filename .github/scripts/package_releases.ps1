param(
    [Parameter(Mandatory = $true)][string]$EngineExe,
    [Parameter(Mandatory = $true)][string]$EngineName,
    [Parameter(Mandatory = $true)][string]$ProductName
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$catalogPath = Join-Path $repoRoot '.games-catalog.json'
$catalog = Get-Content -Raw -LiteralPath $catalogPath | ConvertFrom-Json
if (-not $catalog.playables -or $catalog.playables.Count -eq 0) {
    throw 'The catalog has no playable releases.'
}
$releaseConfigPath = Join-Path $repoRoot '.release-games.json'
if (-not (Test-Path -LiteralPath $releaseConfigPath -PathType Leaf)) {
    throw 'The repository has no .release-games.json coverage manifest.'
}
$releaseConfig = Get-Content -Raw -LiteralPath $releaseConfigPath | ConvertFrom-Json
if ($releaseConfig.version -ne 1) {
    throw '.release-games.json must have version 1.'
}
$playables = @($catalog.playables) + @($releaseConfig.data_playables)

function Resolve-RepositoryPath([string]$RelativePath) {
    if ([System.IO.Path]::IsPathRooted($RelativePath)) {
        throw "Published path must be relative: $RelativePath"
    }
    $absolute = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $RelativePath))
    $prefix = $repoRoot + [System.IO.Path]::DirectorySeparatorChar
    if (-not $absolute.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Published path escapes the repository: $RelativePath"
    }
    if (-not (Test-Path -LiteralPath $absolute)) {
        throw "Published path does not exist: $RelativePath"
    }
    return $absolute
}

$seenSlugs = @{}
foreach ($playable in $playables) {
    $slug = [string]$playable.slug
    if ($slug -notmatch '^[a-z0-9]+(?:-[a-z0-9]+)*$') {
        throw "Unsafe playable slug: $slug"
    }
    if ($seenSlugs.ContainsKey($slug)) {
        throw "Duplicate playable slug: $slug"
    }
    $seenSlugs[$slug] = $true
    if (-not $playable.files -or -not $playable.arguments) {
        throw "Playable $slug must provide files and arguments."
    }
    foreach ($publishedPath in $playable.files) {
        Resolve-RepositoryPath ([string]$publishedPath) | Out-Null
    }
}

$coveredDirectories = @{}
foreach ($playable in $playables) {
    foreach ($publishedPath in $playable.files) {
        $coveredDirectories[([string]$publishedPath).Replace('\', '/').TrimEnd('/')] = $true
    }
}
foreach ($native in @($releaseConfig.native_playables)) {
    $directory = ([string]$native.directory).Replace('\', '/').TrimEnd('/')
    Resolve-RepositoryPath $directory | Out-Null
    $coveredDirectories[$directory] = $true
}
foreach ($gameRoot in @($releaseConfig.game_roots)) {
    $rootRelative = ([string]$gameRoot).Replace('\', '/').TrimEnd('/')
    $rootPath = Resolve-RepositoryPath $rootRelative
    foreach ($directory in Get-ChildItem -LiteralPath $rootPath -Directory) {
        $relative = "$rootRelative/$($directory.Name)"
        if (-not $coveredDirectories.ContainsKey($relative)) {
            throw "Game directory has no Windows release definition: $relative"
        }
    }
}

$enginePath = [System.IO.Path]::GetFullPath($EngineExe)
if (-not (Test-Path -LiteralPath $enginePath -PathType Leaf)) {
    throw "Engine executable does not exist: $enginePath"
}

$dist = Join-Path $repoRoot 'dist'
if (Test-Path -LiteralPath $dist) {
    Remove-Item -LiteralPath $dist -Recurse -Force
}
New-Item -ItemType Directory -Path $dist | Out-Null

$tempRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [System.IO.Path]::GetTempPath() }
$launcherBuild = Join-Path $tempRoot "games-launcher-build-$($ProductName.ToLowerInvariant())"
if (Test-Path -LiteralPath $launcherBuild) {
    Remove-Item -LiteralPath $launcherBuild -Recurse -Force
}
New-Item -ItemType Directory -Path $launcherBuild | Out-Null
$launcher = Join-Path $launcherBuild 'Play.exe'
& rustc (Join-Path $PSScriptRoot 'play_launcher.rs') -O -o $launcher
if ($LASTEXITCODE -ne 0) {
    throw "rustc failed to build the launcher with exit code $LASTEXITCODE"
}

$notes = @(
    "Windows x64 playable builds from ``$($catalog.source_repository)`` at ``$($catalog.source_revision)``.",
    '',
    'Download a ZIP, extract it, and double-click the included `Play-*.exe` file.',
    '',
    '> These executables are not code-signed, so Windows may show a SmartScreen warning.',
    '',
    '## Included builds',
    ''
)

foreach ($playable in $playables) {
    $slug = [string]$playable.slug
    if ($slug -notmatch '^[a-z0-9]+(?:-[a-z0-9]+)*$') {
        throw "Unsafe playable slug: $slug"
    }
    $stage = Join-Path $tempRoot "games-release-$slug"
    if (Test-Path -LiteralPath $stage) {
        Remove-Item -LiteralPath $stage -Recurse -Force
    }
    New-Item -ItemType Directory -Path $stage | Out-Null
    Copy-Item -LiteralPath $enginePath -Destination (Join-Path $stage $EngineName)
    Copy-Item -LiteralPath $launcher -Destination (Join-Path $stage "Play-$slug.exe")
    Set-Content -LiteralPath (Join-Path $stage 'engine.name') -Value $EngineName -Encoding utf8NoBOM
    Set-Content -LiteralPath (Join-Path $stage 'launch.args') -Value @($playable.arguments) -Encoding utf8NoBOM

    foreach ($publishedPath in $playable.files) {
        $relative = ([string]$publishedPath).Replace('/', [System.IO.Path]::DirectorySeparatorChar)
        $source = Resolve-RepositoryPath $relative
        $destination = Join-Path (Join-Path $stage 'content') $relative
        New-Item -ItemType Directory -Path (Split-Path $destination -Parent) -Force | Out-Null
        Copy-Item -LiteralPath $source -Destination $destination -Recurse
    }

    $readme = @(
        [string]$playable.name,
        ('=' * ([string]$playable.name).Length),
        '',
        "Double-click Play-$slug.exe to start.",
        '',
        "Built from $($catalog.source_repository) commit $($catalog.source_revision).",
        'This build is not code-signed; Windows may display a SmartScreen warning.'
    )
    Set-Content -LiteralPath (Join-Path $stage 'README.txt') -Value $readme -Encoding utf8NoBOM

    $archive = Join-Path $dist "$slug-windows-x64.zip"
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive -CompressionLevel Optimal
    Remove-Item -LiteralPath $stage -Recurse -Force
    $notes += "- **$($playable.name)** — ``$([System.IO.Path]::GetFileName($archive))``"
}

Remove-Item -LiteralPath $launcherBuild -Recurse -Force
$checksumLines = Get-ChildItem -LiteralPath $dist -Filter '*.zip' | Sort-Object Name | ForEach-Object {
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
    "$hash  $($_.Name)"
}
Set-Content -LiteralPath (Join-Path $dist 'SHA256SUMS.txt') -Value $checksumLines -Encoding ascii
Set-Content -LiteralPath (Join-Path $dist 'release-notes.md') -Value $notes -Encoding utf8NoBOM

Write-Output "Packaged $($playables.Count) $ProductName data-driven playable build(s)."
