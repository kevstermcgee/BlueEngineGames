param(
    [Parameter(Mandatory = $true)][string]$EngineRoot,
    [switch]$ValidateOnly,
    [string]$GameSlug = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$engineRootPath = [System.IO.Path]::GetFullPath($EngineRoot)
$configPath = Join-Path $repoRoot '.release-games.json'
$config = Get-Content -Raw -LiteralPath $configPath | ConvertFrom-Json
$nativePlayables = @($config.native_playables)

& python (Join-Path $repoRoot 'distribution/check_catalog.py') --root $repoRoot
if ($LASTEXITCODE -ne 0) { throw 'Windows download coverage validation failed.' }

if ($nativePlayables.Count -eq 0) {
    throw 'The release manifest has no native playable releases.'
}

$seenSlugs = @{}
foreach ($game in $nativePlayables) {
    $slug = [string]$game.slug
    if ($slug -notmatch '^[a-z0-9]+(?:-[a-z0-9]+)*$') {
        throw "Unsafe native playable slug: $slug"
    }
    if ($seenSlugs.ContainsKey($slug)) {
        throw "Duplicate native playable slug: $slug"
    }
    $seenSlugs[$slug] = $true
    if ([string]$game.kind -notin 'cargo', 'cargo-package', 'engine-sandbox', 'release-asset') {
        throw "Unknown native playable kind for ${slug}: $($game.kind)"
    }
    $directory = [System.IO.Path]::GetFullPath((Join-Path $repoRoot ([string]$game.directory)))
    $prefix = $repoRoot + [System.IO.Path]::DirectorySeparatorChar
    if (-not $directory.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not (Test-Path -LiteralPath $directory -PathType Container)) {
        throw "Native playable directory is missing or unsafe: $($game.directory)"
    }
    if ($game.kind -notin 'engine-sandbox', 'release-asset') {
        if (-not $game.binary -or -not (Test-Path -LiteralPath (Join-Path $directory 'Cargo.toml') -PathType Leaf)) {
            throw "Native Cargo playable $slug needs a binary and Cargo.toml."
        }
        if ($game.kind -eq 'cargo-package' -and
            -not (Test-Path -LiteralPath (Join-Path $directory 'scripts\ship.py') -PathType Leaf)) {
            throw "Native packaged playable $slug has no scripts/ship.py."
        }
    }
}

if ($GameSlug) {
    $nativePlayables = @($nativePlayables | Where-Object slug -eq $GameSlug)
    if ($nativePlayables.Count -ne 1) { throw "Unknown native game selection: $GameSlug" }
}

if ($ValidateOnly) {
    Write-Output "Validated $($nativePlayables.Count) native Windows release definition(s)."
    exit 0
}

if (-not (Test-Path -LiteralPath (Join-Path $engineRootPath 'Cargo.toml') -PathType Leaf)) {
    throw "BlueEngine checkout does not exist: $engineRootPath"
}
$dist = Join-Path $repoRoot 'dist'
if (-not (Test-Path -LiteralPath $dist -PathType Container)) {
    throw 'dist must be created by package_releases.ps1 before native games are packaged.'
}

$tempRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [System.IO.Path]::GetTempPath() }
$expectedEngine = [System.IO.Path]::GetFullPath((Join-Path $repoRoot '..\BlueEngine'))
$createdJunction = $false
$originalManifests = @{}
try {
    if (-not $expectedEngine.Equals($engineRootPath, [System.StringComparison]::OrdinalIgnoreCase)) {
        if (Test-Path -LiteralPath $expectedEngine) {
            throw "The path-dependency location already exists and is not this checkout: $expectedEngine"
        }
        New-Item -ItemType Junction -Path $expectedEngine -Target $engineRootPath | Out-Null
        $createdJunction = $true
    }

    foreach ($game in $nativePlayables) {
        $slug = [string]$game.slug
        $name = [string]$game.name
        $kind = [string]$game.kind
        $directory = [System.IO.Path]::GetFullPath((Join-Path $repoRoot ([string]$game.directory)))
        if ($kind -notin 'engine-sandbox', 'release-asset') {
            # Exported games can retain either authoring-relative paths or old git pins.
            # Bind both to the catalog engine, preserving the remaining lockfile exactly.
            $manifest = Join-Path $directory 'Cargo.toml'
            $originalManifests[$manifest] = [IO.File]::ReadAllBytes($manifest)
            $lock = Join-Path $directory 'Cargo.lock'
            if (Test-Path -LiteralPath $lock -PathType Leaf) {
                $originalManifests[$lock] = [IO.File]::ReadAllBytes($lock)
            }
            & python (Join-Path $repoRoot 'distribution/bind_engine.py') --manifest $manifest --engine $engineRootPath
            if ($LASTEXITCODE -ne 0) { throw "$name catalog engine binding failed with exit code $LASTEXITCODE" }
            $metadataText = & cargo metadata --locked --format-version 1 --manifest-path $manifest
            if ($LASTEXITCODE -ne 0) { throw "$name locked dependency verification failed with exit code $LASTEXITCODE" }
            $metadata = ($metadataText -join "`n") | ConvertFrom-Json
            $engines = @($metadata.packages | Where-Object name -eq 'be2')
            $expectedManifest = Join-Path $engineRootPath 'Cargo.toml'
            if ($engines.Count -ne 1 -or $null -ne $engines[0].source -or
                -not [IO.Path]::GetFullPath([string]$engines[0].manifest_path).Equals(
                    $expectedManifest, [StringComparison]::OrdinalIgnoreCase)) {
                throw "$name resolved an engine outside the catalog engine checkout."
            }
        }
        $stage = Join-Path $tempRoot "games-native-release-$slug"
        if (Test-Path -LiteralPath $stage) {
            Remove-Item -LiteralPath $stage -Recurse -Force
        }

        if ($kind -eq 'release-asset') {
            & python (Join-Path $repoRoot 'distribution/fetch_release.py') --source (Join-Path $directory 'release-source.json') --output $stage
            if ($LASTEXITCODE -ne 0) { throw "$name pinned release download failed with exit code $LASTEXITCODE" }
            Copy-Item (Join-Path $directory 'README.md') (Join-Path $stage 'README.md')
        } elseif ($kind -eq 'engine-sandbox') {
            # package_sandbox.py reads <engine>/target/release, so this game keeps the engine's own target
            # directory (already warm from the engine build step) instead of the shared one below.
            $sharedTarget = $env:CARGO_TARGET_DIR
            Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
            try {
                & cargo build --release --locked --manifest-path (Join-Path $engineRootPath 'Cargo.toml') --bin blueengine-sandbox --bin be2 --bin be2-tools
                if ($LASTEXITCODE -ne 0) { throw "BlueEngineSandbox build failed with exit code $LASTEXITCODE" }
                & python (Join-Path $engineRootPath 'scripts\package_sandbox.py') --output $stage
                if ($LASTEXITCODE -ne 0) { throw "BlueEngineSandbox packaging failed with exit code $LASTEXITCODE" }
            } finally {
                if ($sharedTarget) { $env:CARGO_TARGET_DIR = $sharedTarget }
            }
        } elseif ($kind -eq 'cargo-package') {
            Push-Location $directory
            $originalToolingRoot = $env:BE2_TOOLING_ROOT
            try {
                # Exported launchers retain authoring-relative pins. Select the same
                # catalog engine as Cargo without changing their verified tooling hash.
                $env:BE2_TOOLING_ROOT = $engineRootPath
                & python scripts\ship.py package
                if ($LASTEXITCODE -ne 0) { throw "$name packaging failed with exit code $LASTEXITCODE" }
            } finally {
                if ($null -eq $originalToolingRoot) {
                    Remove-Item Env:BE2_TOOLING_ROOT -ErrorAction SilentlyContinue
                } else {
                    $env:BE2_TOOLING_ROOT = $originalToolingRoot
                }
                Pop-Location
            }
            New-Item -ItemType Directory -Path $stage | Out-Null
            Copy-Item -Path (Join-Path $directory 'dist\*') -Destination $stage -Recurse
        } else {
            & cargo build --release --locked --manifest-path (Join-Path $directory 'Cargo.toml') --bin ([string]$game.binary)
            if ($LASTEXITCODE -ne 0) { throw "$name build failed with exit code $LASTEXITCODE" }
            New-Item -ItemType Directory -Path $stage | Out-Null
            # With a shared CARGO_TARGET_DIR every game's output lands in one place.
            $targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $directory 'target' }
            $builtExe = Join-Path $targetRoot "release\$($game.binary).exe"
            if (-not (Test-Path -LiteralPath $builtExe -PathType Leaf)) {
                throw "Built executable is missing: $builtExe"
            }
            Copy-Item -LiteralPath $builtExe -Destination (Join-Path $stage "Play-$slug.exe")
            if (Test-Path -LiteralPath (Join-Path $directory 'README.md')) {
                Copy-Item -LiteralPath (Join-Path $directory 'README.md') -Destination (Join-Path $stage 'README.md')
            }
        }

        $archive = Join-Path $dist "$slug-windows-x64.zip"
        if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive -Force }
        Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive -CompressionLevel Optimal
        Remove-Item -LiteralPath $stage -Recurse -Force
        Add-Content -LiteralPath (Join-Path $dist 'release-notes.md') -Value "- **$name** — ``$([System.IO.Path]::GetFileName($archive))``" -Encoding utf8NoBOM
    }
} finally {
    foreach ($manifest in $originalManifests.Keys) {
        [IO.File]::WriteAllBytes($manifest, $originalManifests[$manifest])
    }
    if ($createdJunction -and (Test-Path -LiteralPath $expectedEngine)) {
        Remove-Item -LiteralPath $expectedEngine -Force
    }
}

$checksumLines = Get-ChildItem -LiteralPath $dist -Filter '*.zip' | Sort-Object Name | ForEach-Object {
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
    "$hash  $($_.Name)"
}
Set-Content -LiteralPath (Join-Path $dist 'SHA256SUMS.txt') -Value $checksumLines -Encoding ascii
Write-Output "Packaged $($nativePlayables.Count) native BlueEngine playable build(s)."
