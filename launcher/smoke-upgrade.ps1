# Starts a real CI-only older app, then verifies a signed NSIS replacement/restart.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$version = (Get-Content launcher/package.json | ConvertFrom-Json).version
$exe = (Resolve-Path 'launcher/src-tauri/target/release/blueengine-games-launcher.exe').Path
$installer = Get-ChildItem launcher/src-tauri/target/release/bundle/nsis/*.exe | Select-Object -First 1
if (-not $installer -or -not (Test-Path "$($installer.FullName).sig")) { throw 'Signed production NSIS artifact is missing' }
$production = Join-Path $env:RUNNER_TEMP 'BlueEngine-Games-production.exe'
$expectedHash = (Get-FileHash $production -Algorithm SHA256).Hash
$env:BLUEENGINE_UPGRADE_INSTALLER = $installer.FullName
$env:BLUEENGINE_UPGRADE_VERSION = $version
New-Item launcher-smoke -ItemType Directory -Force | Out-Null
$feedScript = Join-Path $PSScriptRoot 'smoke-upgrade.mjs'
$server = Start-Process node -ArgumentList @("`"$feedScript`"", 'serve') -PassThru -RedirectStandardOutput (Join-Path $root 'launcher-smoke/upgrade-server.log') -RedirectStandardError (Join-Path $root 'launcher-smoke/upgrade-server-error.log')
$app = $null
try {
  $feedReady = $false
  for ($i = 0; $i -lt 60; $i++) {
    $server.Refresh()
    if ($server.HasExited) { throw 'CI signed update feed exited before readiness' }
    try {
      $health = Invoke-RestMethod 'http://127.0.0.1:9224/health'
      if ($health.version -eq $version) { $feedReady = $true; break }
    } catch {}
    Start-Sleep -Milliseconds 250
  }
  if (-not $feedReady) { throw 'CI signed update feed did not become ready' }
  $app = Start-Process -FilePath $exe -PassThru
  $ready = $false
  for ($i = 0; $i -lt 60; $i++) {
    Start-Sleep -Seconds 1
    $app.Refresh()
    if ($app.HasExited) { throw 'Launcher exited before opening its window' }
    if ($app.MainWindowHandle -ne 0 -and $app.MainWindowTitle -eq 'BlueEngine Games') { $ready = $true; break }
  }
  if (-not $ready) { throw 'Launcher window did not become ready' }
  node launcher/smoke-windows.mjs
  if ($LASTEXITCODE -ne 0) { throw 'Real Windows library/native installation smoke failed' }
  node launcher/smoke-upgrade.mjs click
  if ($LASTEXITCODE -ne 0) { throw 'Automatic update detection or user update action failed' }
  if (-not $app.WaitForExit(180000)) { throw 'Signed updater did not exit the previous launcher' }
  $restarted = $null
  for ($i = 0; $i -lt 180; $i++) {
    $candidates = @(Get-Process -Name 'blueengine-games-launcher' -ErrorAction SilentlyContinue | Where-Object { $_.Id -ne $app.Id })
    foreach ($candidate in $candidates) {
      if ($candidate.MainWindowHandle -ne 0 -and $candidate.Path -and (Get-FileHash $candidate.Path -Algorithm SHA256).Hash -eq $expectedHash) {
        $restarted = $candidate
        break
      }
    }
    if ($restarted) { break }
    Start-Sleep -Seconds 1
  }
  if (-not $restarted) { throw 'Signed installer did not restart the exact production executable' }
  $before = Get-Content launcher-smoke/upgrade-before.json | ConvertFrom-Json
  if ((Get-Content $before.marker -Raw) -ne "Keep player files across a signed launcher upgrade`n") { throw 'Launcher upgrade changed the player save marker' }
  $health = Invoke-RestMethod 'http://127.0.0.1:9224/health'
  if ($health.manifestRequests -lt 1 -or $health.installerRequests -ne 2) { throw 'Upgrade did not reject the corrupt download and then retry the genuine signed artifact' }
  $receipt = @{version=$version; previous_pid=$app.Id; restarted_pid=$restarted.Id; executable=$restarted.Path; sha256=$expectedHash; player_file_preserved=$true; corrupt_signature_rejected=$true; signed_feed=$health}
  $receipt | ConvertTo-Json -Depth 5 | Set-Content launcher-smoke/upgrade-result.json -Encoding utf8NoBOM
  Write-Host "Verified automatic detection, user-triggered signed update, old-process exit and production $version restart."
} finally {
  Get-Process -Name 'blueengine-games-launcher' -ErrorAction SilentlyContinue | Stop-Process -Force
  $server.Refresh()
  if (-not $server.HasExited) { Stop-Process -Id $server.Id -Force }
}
