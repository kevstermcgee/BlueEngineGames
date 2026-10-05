param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '../dist'),
    [string]$GameSlug = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output = [System.IO.Path]::GetFullPath($OutputDirectory)
$repository = 'kevstermcgee/BlueEngineGames'
$release = Get-Content -Raw -LiteralPath (Join-Path $repoRoot '.release-games.json') | ConvertFrom-Json
$catalog = Get-Content -Raw -LiteralPath (Join-Path $repoRoot '.games-catalog.json') | ConvertFrom-Json
$safeRepo = $repoRoot.Replace('\', '/')

New-Item -ItemType Directory -Path $output -Force | Out-Null

$definitions = @{}
foreach ($playable in @($catalog.playables) + @($release.data_playables)) {
    foreach ($file in @($playable.files)) {
        $parts = ([string]$file).Replace('\', '/').Trim('/').Split('/')
        if ($parts.Count -ge 2) {
            $key = "$($parts[0])/$($parts[1])"
            if (-not $definitions.ContainsKey($key)) {
                $definitions[$key] = [pscustomobject]@{
                    slug = [string]$playable.slug
                    name = [string]$playable.name
                    kind = 'data game'
                }
            }
        }
    }
}
foreach ($native in @($release.native_playables)) {
    $key = ([string]$native.directory).Replace('\', '/').Trim('/')
    $definitions[$key] = [pscustomobject]@{
        slug = [string]$native.slug
        name = [string]$native.name
        kind = 'native game'
    }
}

function One-Line([object]$Value) {
    return ([string]$Value).Replace([char]9, ' ').Replace([char]13, ' ').Replace([char]10, ' ').Replace('*', '').Trim()
}

function Read-Description([string]$Directory, [string]$Fallback) {
    $identityPath = Join-Path $Directory 'assets\identity.json'
    if (Test-Path -LiteralPath $identityPath) {
        $identity = Get-Content -Raw -LiteralPath $identityPath | ConvertFrom-Json
        if ($identity.tagline) { return One-Line $identity.tagline }
    }
    $cargoPath = Join-Path $Directory 'Cargo.toml'
    if (Test-Path -LiteralPath $cargoPath) {
        $cargoText = Get-Content -Raw -LiteralPath $cargoPath
        if ($cargoText -match '(?m)^description\s*=\s*"([^"]+)"') { return One-Line $Matches[1] }
    }
    $readmePath = Join-Path $Directory 'README.md'
    if (Test-Path -LiteralPath $readmePath) {
        $paragraph = @()
        foreach ($line in Get-Content -LiteralPath $readmePath -Encoding utf8) {
            $trimmed = $line.Trim()
            if (-not $trimmed -or $trimmed.StartsWith('#') -or $trimmed.StartsWith('![')) {
                if ($paragraph.Count -gt 0) { break }
                continue
            }
            $paragraph += $trimmed
        }
        if ($paragraph.Count -gt 0) { return One-Line ($paragraph -join ' ') }
    }
    return $Fallback
}

function Game-Version([string]$Directory) {
    $releaseSource = Join-Path $Directory 'release-source.json'
    if (Test-Path $releaseSource) { return [string](Get-Content -Raw $releaseSource | ConvertFrom-Json).version }
    $cargoPath = Join-Path $Directory 'Cargo.toml'
    if (Test-Path -LiteralPath $cargoPath) {
        $text = Get-Content -Raw -LiteralPath $cargoPath
        if ($text -match '(?m)^version\s*=\s*"([^"]+)"') { return $Matches[1] }
    }
    $gamePath = Join-Path $Directory 'game.json'
    if (Test-Path -LiteralPath $gamePath) {
        $game = Get-Content -Raw -LiteralPath $gamePath | ConvertFrom-Json
        if ($game.PSObject.Properties.Name -contains 'version') { return [string]$game.version }
        if ($game.PSObject.Properties.Name -contains 'game') { return "schema $($game.game)" }
    }
    return 'catalog'
}

function Engine-Version([string]$Directory) {
    $releaseSource = Join-Path $Directory 'release-source.json'
    if (Test-Path $releaseSource) { return [string](Get-Content -Raw $releaseSource | ConvertFrom-Json).engine_revision }
    $gamePath = Join-Path $Directory 'game.json'
    if (Test-Path -LiteralPath $gamePath) {
        $game = Get-Content -Raw -LiteralPath $gamePath | ConvertFrom-Json
        if ($game.PSObject.Properties.Name -contains 'engine' -and
            $game.engine.PSObject.Properties.Name -contains 'ref' -and $game.engine.ref) {
            $ref = [string]$game.engine.ref
            return $ref.Substring(0, [Math]::Min(12, $ref.Length))
        }
    }
    # Identity records the author's starting engine. Path-based games are rebuilt
    # against this release's catalog engine, so report the actual build revision.
    $cargoPath = Join-Path $Directory 'Cargo.toml'
    if (Test-Path -LiteralPath $cargoPath) {
        $text = Get-Content -Raw -LiteralPath $cargoPath
        if ($text -match 'rev\s*=\s*"([0-9a-fA-F]{7,40})"') {
            return $Matches[1].Substring(0, [Math]::Min(12, $Matches[1].Length))
        }
    }
    $sourceRevision = [string]$catalog.source_revision
    return $sourceRevision.Substring(0, [Math]::Min(12, $sourceRevision.Length))
}

$releaseTag = $env:RELEASE_TAG
if (-not $releaseTag) { $releaseTag = "games-$((& git -C $repoRoot rev-parse HEAD).Substring(0, 12))" }
$fixtures = @()
if ($release.PSObject.Properties.Name -contains 'non_playable_directories') { $fixtures = @($release.non_playable_directories) }
$rows = @()
foreach ($gameRoot in @($release.game_roots)) {
    $rootRelative = ([string]$gameRoot).Replace('\', '/').Trim('/')
    $rootPath = Join-Path $repoRoot $rootRelative
    foreach ($directory in Get-ChildItem -LiteralPath $rootPath -Directory | Sort-Object Name) {
        $relative = "$rootRelative/$($directory.Name)"
        if ($relative -in $fixtures) { continue }
        if (-not $definitions.ContainsKey($relative)) {
            throw "Release discovery found a game without a release definition: $relative"
        }
        $definition = $definitions[$relative]
        if ($GameSlug -and $definition.slug -ne $GameSlug) { continue }
        $createdLines = @(& git -c "safe.directory=$safeRepo" -C $repoRoot log --diff-filter=A --format=%aI -- $relative 2>$null)
        $createdDates = @($createdLines | ForEach-Object { [DateTimeOffset]$_ } | Sort-Object UtcDateTime)
        $created = if ($createdDates.Count -gt 0) { $createdDates[0].ToString('yyyy-MM-dd') } else { 'unknown' }
        $fallback = "$($definition.name), built with BlueEngine."
        $description = Read-Description $directory.FullName $fallback
        $asset = "https://github.com/$repository/releases/download/$releaseTag/$($definition.slug)-windows-x64.zip"
        $fields = @(
            (One-Line $definition.slug)
            (One-Line $definition.name)
            (One-Line $description)
            $created
            (One-Line (Game-Version $directory.FullName))
            (One-Line (Engine-Version $directory.FullName))
            (One-Line $definition.kind)
            $asset
            $releaseTag
            $(if (Test-Path (Join-Path $repoRoot "dist/$($definition.slug)-windows-x64.zip")) {
                (Get-FileHash -Algorithm SHA256 (Join-Path $repoRoot "dist/$($definition.slug)-windows-x64.zip")).Hash.ToLowerInvariant()
            } else { '' })
        )
        $rows += $fields -join [char]9
    }
}

$header = @('slug', 'name', 'description', 'created', 'game_version', 'engine_version', 'kind', 'asset', 'release', 'sha256') -join [char]9
if ($GameSlug -and $rows.Count -ne 1) { throw "Selection has no game page: $GameSlug" }
Set-Content -LiteralPath (Join-Path $output 'Games-catalog.tsv') -Value (@($header) + $rows) -Encoding utf8NoBOM
Write-Output "Cataloged $($rows.Count) game(s) for release $releaseTag."
