$ErrorActionPreference = 'Stop'
$gameRoot = $PSScriptRoot
& (Join-Path $gameRoot 'scripts\blue.ps1') ship
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host 'Wrecking Orbit is packaged and its Desktop shortcut is ready.' -ForegroundColor Green
