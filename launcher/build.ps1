param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'build')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output = [System.IO.Path]::GetFullPath($OutputDirectory)
$config = Get-Content -Raw -LiteralPath (Join-Path $repoRoot '.launcher-config.json') | ConvertFrom-Json
$release = Get-Content -Raw -LiteralPath (Join-Path $repoRoot '.release-games.json') | ConvertFrom-Json
$catalog = Get-Content -Raw -LiteralPath (Join-Path $repoRoot '.games-catalog.json') | ConvertFrom-Json
$safeRepo = $repoRoot.Replace('\', '/')

if (Test-Path -LiteralPath $output) {
    Remove-Item -LiteralPath $output -Recurse -Force
}
New-Item -ItemType Directory -Path $output | Out-Null

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
    $gamePath = Join-Path $Directory 'game.json'
    if (Test-Path -LiteralPath $gamePath) {
        $game = Get-Content -Raw -LiteralPath $gamePath | ConvertFrom-Json
        if ($game.PSObject.Properties.Name -contains 'engine' -and
            $game.engine.PSObject.Properties.Name -contains 'ref' -and $game.engine.ref) {
            $ref = [string]$game.engine.ref
            return $ref.Substring(0, [Math]::Min(12, $ref.Length))
        }
    }
    $identityPath = Join-Path $Directory 'assets\identity.json'
    if (Test-Path -LiteralPath $identityPath) {
        $identity = Get-Content -Raw -LiteralPath $identityPath | ConvertFrom-Json
        if ($identity.PSObject.Properties.Name -contains 'engine_revision' -and $identity.engine_revision) {
            return [string]$identity.engine_revision
        }
    }
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

$rows = @()
foreach ($gameRoot in @($release.game_roots)) {
    $rootRelative = ([string]$gameRoot).Replace('\', '/').Trim('/')
    $rootPath = Join-Path $repoRoot $rootRelative
    foreach ($directory in Get-ChildItem -LiteralPath $rootPath -Directory | Sort-Object Name) {
        $relative = "$rootRelative/$($directory.Name)"
        if (-not $definitions.ContainsKey($relative)) {
            throw "Launcher discovery found a game without a release definition: $relative"
        }
        $definition = $definitions[$relative]
        $createdLines = @(& git -c "safe.directory=$safeRepo" -C $repoRoot log --diff-filter=A --format=%aI -- $relative 2>$null)
        $createdDates = @($createdLines | ForEach-Object { [DateTimeOffset]$_ } | Sort-Object UtcDateTime)
        $created = if ($createdDates.Count -gt 0) { $createdDates[0].ToString('yyyy-MM-dd') } else { 'unknown' }
        $fallback = "$($definition.name), built with $($config.engine_name)."
        $description = Read-Description $directory.FullName $fallback
        $asset = "https://github.com/$($config.repository)/releases/latest/download/$($definition.slug)-windows-x64.zip"
        $fields = @(
            (One-Line $definition.slug)
            (One-Line $definition.name)
            (One-Line $description)
            $created
            (One-Line (Game-Version $directory.FullName))
            (One-Line (Engine-Version $directory.FullName))
            (One-Line $definition.kind)
            $asset
        )
        $rows += $fields -join [char]9
    }
}

$separator = [char]9
$settings = @()
$settings += "product${separator}$($config.product)"
$settings += "repository${separator}$($config.repository)"
$settings += "install_folder${separator}$($config.install_folder)"
$settings += "catalog_url${separator}https://github.com/$($config.repository)/releases/latest/download/$($config.output_name)-catalog.tsv"
$settings += "accent${separator}$($config.accent)"
$header = @('slug', 'name', 'description', 'created', 'game_version', 'engine_version', 'kind', 'asset') -join [char]9
Set-Content -LiteralPath (Join-Path $output 'launcher-settings.tsv') -Value $settings -Encoding utf8
Set-Content -LiteralPath (Join-Path $output 'launcher-catalog.tsv') -Value (@($header) + $rows) -Encoding utf8

$cscCandidates = @(
    (Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'),
    (Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe')
)
$csc = $cscCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if (-not $csc) { throw 'The Windows .NET Framework C# compiler was not found.' }
$exe = Join-Path $output ([string]$config.launcher_exe)
& $csc /nologo /target:winexe "/out:$exe" /reference:System.dll /reference:System.Core.dll /reference:System.Drawing.dll /reference:System.Windows.Forms.dll /reference:System.IO.Compression.dll /reference:System.IO.Compression.FileSystem.dll (Join-Path $PSScriptRoot 'EngineGamesLauncher.cs')
if ($LASTEXITCODE -ne 0) { throw "Launcher compilation failed with exit code $LASTEXITCODE" }

$readme = @(
    "$($config.product)",
    ('=' * ([string]$config.product).Length),
    '',
    "Run $($config.launcher_exe). Games are downloaded from the latest $($config.repository) GitHub release",
    'and installed per-user under LocalAppData. Downloads and executables are not code-signed.'
)
Set-Content -LiteralPath (Join-Path $output 'README.txt') -Value $readme -Encoding utf8
Write-Output "Built $($config.launcher_exe) with $($rows.Count) automatically discovered game(s)."
