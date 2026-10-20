param(
    [string]$DistDirectory = (Join-Path $PSScriptRoot '../dist'),
    [string]$GameSlug = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = Split-Path $PSScriptRoot
$dist = [IO.Path]::GetFullPath($DistDirectory)
$rows = @(Get-Content -Raw (Join-Path $dist 'Games-catalog.tsv') | ConvertFrom-Csv -Delimiter "`t")
& python (Join-Path $PSScriptRoot 'package_metadata.py') audit --dist $dist --output (Join-Path $dist 'portable-metadata-proof.json')
if ($LASTEXITCODE -ne 0) { throw 'Shipped ZIP, playable executable and installer metadata audit failed.' }
if (-not $GameSlug) {
    $preferred = @($rows | Where-Object slug -eq 'corkscrew-key')
    $GameSlug = if ($preferred.Count -eq 1) { 'corkscrew-key' } else { [string]$rows[0].slug }
}
if ($GameSlug -notmatch '^[a-z0-9]+(?:-[a-z0-9]+)*$' -or @($rows | Where-Object slug -eq $GameSlug).Count -ne 1) {
    throw 'Selected installer has no unambiguous game package.'
}
$temporary = Join-Path ([IO.Path]::GetTempPath()) "portable-game-install-$([Guid]::NewGuid().ToString('N'))"
$install = Join-Path $temporary 'installed'
$setup = Join-Path $dist "$GameSlug-setup-windows-x64.exe"
try {
    New-Item -ItemType Directory -Path $temporary | Out-Null
    $process = Start-Process -FilePath $setup -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',"/DIR=`"$install`"") -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Metadata-bearing installer exited with $($process.ExitCode)." }
    & python (Join-Path $PSScriptRoot 'package_metadata.py') audit --dist $dist --slug $GameSlug --installed $install --output (Join-Path $dist "portable-installed-$GameSlug-proof.json")
    if ($LASTEXITCODE -ne 0) { throw 'Installed game lost or changed its embedded metadata.' }
    Write-Output "Installed and verified portable game metadata for $GameSlug."
} finally {
    $uninstaller = Join-Path $install 'unins000.exe'
    if (Test-Path -LiteralPath $uninstaller) {
        $process = Start-Process -FilePath $uninstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "Portable game test uninstall exited with $($process.ExitCode)." }
    }
    if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force }
}
