param(
    [string]$DistDirectory = (Join-Path $PSScriptRoot '../../dist'),
    [switch]$PreserveArchives
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$dist = [IO.Path]::GetFullPath($DistDirectory)
$catalog = Get-Content -Raw (Join-Path $dist 'Games-catalog.tsv') | ConvertFrom-Csv -Delimiter "`t"
$definitions = @($catalog | ForEach-Object {
    [pscustomobject]@{ slug = $_.slug; name = $_.name; exe = ''; folder = "BlueEngine\Games\$($_.slug)" }
})
$csc = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
$updater = Join-Path ([IO.Path]::GetTempPath()) "blueengine-updater-$([Guid]::NewGuid().ToString('N')).exe"
& $csc /nologo /target:winexe "/out:$updater" /reference:System.dll /reference:System.Core.dll /reference:System.Drawing.dll /reference:System.Windows.Forms.dll /reference:System.IO.Compression.dll /reference:System.IO.Compression.FileSystem.dll (Join-Path $repoRoot 'distribution/GameUpdater.cs')
if ($LASTEXITCODE -ne 0) { throw 'Could not compile game updater.' }
$iscc = Get-Command ISCC.exe -ErrorAction SilentlyContinue
if (-not $iscc) {
    $compiler = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
    if (-not (Test-Path $compiler)) { throw 'Install Inno Setup 6 before packaging installers.' }
} else { $compiler = $iscc.Source }
function Write-Catalog {
    $fields = @('slug', 'name', 'description', 'created', 'game_version', 'engine_version', 'kind', 'asset', 'release', 'sha256')
    $lines = @($fields -join "`t")
    foreach ($row in $catalog) {
        $lines += (@($fields | ForEach-Object { [string]$row.$_ }) -join "`t")
    }
    Set-Content (Join-Path $dist 'Games-catalog.tsv') $lines -Encoding utf8NoBOM
}

foreach ($game in $definitions) {
    $archive = Join-Path $dist "$($game.slug)-windows-x64.zip"
    if (-not (Test-Path $archive)) { throw "No package for $($game.slug)" }
    $stage = Join-Path ([IO.Path]::GetTempPath()) "blueengine-installer-$([Guid]::NewGuid().ToString('N'))"
    try {
        Expand-Archive -LiteralPath $archive -DestinationPath $stage
        if (-not $game.exe) {
            $play = Join-Path $stage "Play-$($game.slug).exe"
            $plain = Join-Path $stage "$($game.slug).exe"
            if (Test-Path $play) { $game.exe = [IO.Path]::GetFileName($play) }
            elseif (Test-Path $plain) { $game.exe = [IO.Path]::GetFileName($plain) }
            elseif (Test-Path (Join-Path $stage 'ship.json')) {
                $game.exe = [string](Get-Content -Raw (Join-Path $stage 'ship.json') | ConvertFrom-Json).exe
            } else {
                $exes = @(Get-ChildItem $stage -Filter '*.exe' | Where-Object Name -notmatch 'server|tools|unins')
                if ($exes.Count -ne 1) { throw "Cannot determine the playable executable for $($game.slug)" }
                $game.exe = $exes[0].Name
            }
        }
        if (-not (Test-Path -LiteralPath (Join-Path $stage $game.exe))) { throw "Missing executable $($game.exe)" }
        Copy-Item $updater (Join-Path $stage "Update-$($game.slug).exe")
        # The update ZIP contains only shipped payload, never local installation receipts.
        # Repack first, then add receipts to the installer staging directory.
        if (-not $PreserveArchives) {
            Remove-Item $archive
            Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive -CompressionLevel Optimal
        }
        ($catalog | Where-Object slug -eq $game.slug).sha256 = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        $manifest = @(Get-ChildItem $stage -Recurse -File | ForEach-Object {
            $relative = $_.FullName.Substring($stage.Length + 1)
            "$relative`t$((Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())"
        })
        Set-Content (Join-Path $stage '.installed-files.tsv') $manifest -Encoding utf8NoBOM
        Set-Content (Join-Path $stage '.installed-release') $env:RELEASE_TAG -Encoding utf8NoBOM
        $icon = Get-ChildItem $stage -Filter '*.ico' | Select-Object -First 1
        $iconArgs = if ($icon) { @("/DAppIcon=$($icon.FullName)") } else { @() }
        & $compiler "/DAppSlug=$($game.slug)" "/DAppName=$($game.name)" "/DAppExe=$($game.exe)" `
                "/DAppFolder=$($game.folder)" "/DSourceDir=$stage" "/DOutputDir=$dist" `
                "/DReleaseTag=$env:RELEASE_TAG" @iconArgs (Join-Path $PSScriptRoot 'game_installer.iss')
        if ($LASTEXITCODE -ne 0) { throw "Installer compilation failed for $($game.slug)" }
    } finally { if (Test-Path $stage) { Remove-Item $stage -Recurse -Force } }
}
Remove-Item $updater

Write-Catalog
$checksumLines = Get-ChildItem $dist -File | Where-Object { $_.Extension -in '.zip', '.exe' -or $_.Name -eq 'Games-catalog.tsv' } |
    Sort-Object Name | ForEach-Object { "$((Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $($_.Name)" }
Set-Content (Join-Path $dist 'SHA256SUMS.txt') $checksumLines -Encoding ascii
Add-Content (Join-Path $dist 'release-notes.md') -Value "`nInstall using the ``*-setup-windows-x64.exe`` downloads. Games install per-user and offer a desktop shortcut. Use the game’s Start Menu Check for updates shortcut or run a newer installer to update an existing installation. Older releases remain available."
Add-Content (Join-Path $dist 'release-notes.md') -Value 'These builds are unsigned. Windows may display a SmartScreen warning.'
