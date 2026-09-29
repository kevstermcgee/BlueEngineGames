param(
    [Parameter(Mandatory = $true)][string]$EngineRoot,
    [switch]$ValidateOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$engineRootPath = [System.IO.Path]::GetFullPath($EngineRoot)
$configPath = Join-Path $repoRoot '.release-games.json'
$config = Get-Content -Raw -LiteralPath $configPath | ConvertFrom-Json
$nativePlayables = @($config.native_playables)

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
    if ([string]$game.kind -notin 'cargo', 'cargo-package', 'engine-sandbox') {
        throw "Unknown native playable kind for ${slug}: $($game.kind)"
    }
    $directory = [System.IO.Path]::GetFullPath((Join-Path $repoRoot ([string]$game.directory)))
    $prefix = $repoRoot + [System.IO.Path]::DirectorySeparatorChar
    if (-not $directory.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not (Test-Path -LiteralPath $directory -PathType Container)) {
        throw "Native playable directory is missing or unsafe: $($game.directory)"
    }
    if ($game.kind -ne 'engine-sandbox') {
        if (-not $game.binary -or -not (Test-Path -LiteralPath (Join-Path $directory 'Cargo.toml') -PathType Leaf)) {
            throw "Native Cargo playable $slug needs a binary and Cargo.toml."
        }
        if ($game.kind -eq 'cargo-package' -and
            -not (Test-Path -LiteralPath (Join-Path $directory 'scripts\ship.py') -PathType Leaf)) {
            throw "Native packaged playable $slug has no scripts/ship.py."
        }
    }
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
        $stage = Join-Path $tempRoot "games-native-release-$slug"
        if (Test-Path -LiteralPath $stage) {
            Remove-Item -LiteralPath $stage -Recurse -Force
        }

        if ($kind -eq 'engine-sandbox') {
            & cargo build --release --locked --manifest-path (Join-Path $engineRootPath 'Cargo.toml') --bin blueengine-sandbox --bin be2 --bin be2-tools
            if ($LASTEXITCODE -ne 0) { throw "BlueEngineSandbox build failed with exit code $LASTEXITCODE" }
            & python (Join-Path $engineRootPath 'scripts\package_sandbox.py') --output $stage
            if ($LASTEXITCODE -ne 0) { throw "BlueEngineSandbox packaging failed with exit code $LASTEXITCODE" }
        } elseif ($kind -eq 'cargo-package') {
            Push-Location $directory
            try {
                & python scripts\ship.py package
                if ($LASTEXITCODE -ne 0) { throw "$name packaging failed with exit code $LASTEXITCODE" }
            } finally {
                Pop-Location
            }
            New-Item -ItemType Directory -Path $stage | Out-Null
            Copy-Item -Path (Join-Path $directory 'dist\*') -Destination $stage -Recurse
        } else {
            & cargo build --release --locked --manifest-path (Join-Path $directory 'Cargo.toml') --bin ([string]$game.binary)
            if ($LASTEXITCODE -ne 0) { throw "$name build failed with exit code $LASTEXITCODE" }
            New-Item -ItemType Directory -Path $stage | Out-Null
            $builtExe = Join-Path $directory "target\release\$($game.binary).exe"
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
