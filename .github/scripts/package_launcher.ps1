param([Parameter(Mandatory = $true)][string]$OutputName)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$dist = Join-Path $repoRoot 'dist'
if (-not (Test-Path -LiteralPath $dist -PathType Container)) {
    throw 'dist must be created by package_releases.ps1 first.'
}
$tempRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [System.IO.Path]::GetTempPath() }
$stage = Join-Path $tempRoot "engine-games-launcher-$([Guid]::NewGuid().ToString('N'))"
& (Join-Path $repoRoot 'launcher\build.ps1') -OutputDirectory $stage
if ($LASTEXITCODE -ne 0) { throw "Launcher build failed with exit code $LASTEXITCODE" }
$archive = Join-Path $dist "$OutputName-windows-x64.zip"
if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive -Force }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive -CompressionLevel Optimal
Copy-Item -LiteralPath (Join-Path $stage 'launcher-catalog.tsv') -Destination (Join-Path $dist "$OutputName-catalog.tsv")
Remove-Item -LiteralPath $stage -Recurse -Force
Add-Content -LiteralPath (Join-Path $dist 'release-notes.md') -Value "- $OutputName - $([IO.Path]::GetFileName($archive))" -Encoding utf8
$checksumLines = Get-ChildItem -LiteralPath $dist -File | Where-Object { $_.Name -like '*.zip' -or $_.Name -like '*-catalog.tsv' } | Sort-Object Name | ForEach-Object {
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
    "$hash  $($_.Name)"
}
Set-Content -LiteralPath (Join-Path $dist 'SHA256SUMS.txt') -Value $checksumLines -Encoding ascii
Write-Output "Packaged $([IO.Path]::GetFileName($archive))."
