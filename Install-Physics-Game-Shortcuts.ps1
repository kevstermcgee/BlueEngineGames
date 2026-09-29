$ErrorActionPreference = 'Stop'
$games = @('skyhook-sprint', 'magnet-mine', 'wrecking-orbit')

foreach ($game in $games) {
    $installer = Join-Path $PSScriptRoot "games\$game\Install-Desktop-Shortcut.ps1"
    Write-Host "Installing $game..." -ForegroundColor Cyan
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installer
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Host 'All three BlueEngine game shortcuts are ready on your Desktop.' -ForegroundColor Green
