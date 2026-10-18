# Starts a real CI-only older app, then verifies a signed NSIS replacement/restart.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$version = (Get-Content launcher/package.json | ConvertFrom-Json).version
$account = Get-Content (Join-Path $env:RUNNER_TEMP 'blueengine-upgrade-user.json') | ConvertFrom-Json
$credential = Import-Clixml (Join-Path $env:RUNNER_TEMP 'blueengine-upgrade-credential.xml')
[System.Diagnostics.Process]::EnterDebugMode()
$exe = Join-Path $account.workdir 'blueengine-games-launcher.exe'
Copy-Item launcher/src-tauri/target/release/blueengine-games-launcher.exe $exe
$installer = Get-ChildItem launcher/src-tauri/target/release/bundle/nsis/*.exe | Select-Object -First 1
if (-not $installer -or -not (Test-Path "$($installer.FullName).sig")) { throw 'Signed production NSIS artifact is missing' }
$production = Join-Path $env:RUNNER_TEMP 'BlueEngine-Games-production.exe'
$expectedHash = (Get-FileHash $production -Algorithm SHA256).Hash
$env:BLUEENGINE_UPGRADE_INSTALLER = $installer.FullName
$env:BLUEENGINE_UPGRADE_VERSION = $version
New-Item launcher-smoke -ItemType Directory -Force | Out-Null
function Checkpoint([string]$message) {
  Write-Host "$([DateTime]::UtcNow.ToString('o')) $message"
}
Add-Type @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class UpgradeToken {
  [DllImport("advapi32.dll", SetLastError=true)]
  static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
  [DllImport("advapi32.dll", SetLastError=true)]
  static extern bool GetTokenInformation(IntPtr token, int kind, out int value, int size, out int returned);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
  [DllImport("user32.dll")] public static extern IntPtr GetShellWindow();
  [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr window);
  public static bool IsElevated(IntPtr process) {
    IntPtr token;
    if (!OpenProcessToken(process, 8, out token)) throw new Win32Exception();
    try {
      int elevation, returned;
      if (!GetTokenInformation(token, 20, out elevation, 4, out returned)) throw new Win32Exception();
      return elevation != 0;
    } finally { CloseHandle(token); }
  }
}
'@
whoami /groups /fo csv | Set-Content launcher-smoke/upgrade-runner-token.log
function Diagnostics([string]$stage) {
  try {
    $processes = @(Get-CimInstance Win32_Process | Where-Object { $_.Name -match 'blueengine-games-launcher|BlueEngine Games|installer|setup|runas|explorer' } | ForEach-Object {
      $entry = @{pid=$_.ProcessId; parent_pid=$_.ParentProcessId; session=$_.SessionId; name=$_.Name; path=$_.ExecutablePath; command=$_.CommandLine}
      try {
        $p = Get-Process -Id $_.ProcessId -ErrorAction Stop
        $entry.window_handle = $p.MainWindowHandle.ToInt64()
        $entry.window_title = $p.MainWindowTitle
        $entry.elevated = [UpgradeToken]::IsElevated($p.Handle)
        if ($entry.path -and (Test-Path -LiteralPath $entry.path)) {
          $entry.sha256 = (Get-FileHash -LiteralPath $entry.path -Algorithm SHA256).Hash
        }
      } catch { $entry.inspection_error = $_.Exception.Message }
      $entry
    })
    $uninstallRoot = "Registry::HKEY_USERS\$($account.sid)\Software"
    $entries = @(Get-ItemProperty "$uninstallRoot\Microsoft\Windows\CurrentVersion\Uninstall\*","$uninstallRoot\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*" -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -eq 'BlueEngine Games' } | Select-Object DisplayName,DisplayVersion,InstallLocation,UninstallString,PSPath)
    $locations = @($entries | ForEach-Object { if ($_.InstallLocation) { $_.InstallLocation.Trim('"') } }) + @((Join-Path $account.local_app_data 'BlueEngine Games'))
    $installed = @($locations | Where-Object { $_ } | Select-Object -Unique | ForEach-Object {
      $path = Join-Path $_ 'blueengine-games-launcher.exe'
      if (Test-Path -LiteralPath $path) {
        @{path=$path; sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash; version=(Get-Item -LiteralPath $path).VersionInfo.ProductVersion}
      }
    })
    try { $feed = Invoke-RestMethod 'http://127.0.0.1:9224/health' -TimeoutSec 3 } catch { $feed = @{error=$_.Exception.Message} }
    @{stage=$stage; utc=[DateTime]::UtcNow.ToString('o'); account=$account; expected_hash=$expectedHash; expected_version=$version; original_executable=$exe; shell_window=[UpgradeToken]::GetShellWindow().ToInt64(); processes=$processes; uninstall_entries=$entries; installed_files=$installed; feed=$feed} | ConvertTo-Json -Depth 8 | Set-Content "launcher-smoke/upgrade-diagnostics-$stage.json" -Encoding utf8NoBOM
  } catch { Write-Warning "Could not capture upgrade diagnostics: $_" }
}
$feedScript = Join-Path $PSScriptRoot 'smoke-upgrade.mjs'
$server = Start-Process node -ArgumentList @("`"$feedScript`"", 'serve') -PassThru -RedirectStandardOutput (Join-Path $root 'launcher-smoke/upgrade-server.log') -RedirectStandardError (Join-Path $root 'launcher-smoke/upgrade-server-error.log')
$app = $null
$wrapper = $null
try {
  Checkpoint 'Starting loopback signed update feed.'
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
  # The early preflight created a genuine standard-user desktop/profile. SAFER
  # runas /trustlevel does not clear TokenElevation on hosted Windows runners.
  Checkpoint "Starting the instrumented launcher as standard user $($account.user)."
  $beforePids = @(Get-Process -Name 'blueengine-games-launcher' -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
  $wrapperCode = @'
$ErrorActionPreference = 'Stop'
$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$sid = $identity.User.Value
$profile = [Environment]::ExpandEnvironmentVariables((Get-ItemProperty -LiteralPath "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\$sid").ProfileImagePath)
$env:USERPROFILE = $profile
$env:LOCALAPPDATA = Join-Path $profile 'AppData\Local'
$env:APPDATA = Join-Path $profile 'AppData\Roaming'
$env:TEMP = Join-Path $env:LOCALAPPDATA 'Temp'
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Path $env:TEMP -Force | Out-Null
$app = Start-Process -FilePath '__APP_EXE__' -WorkingDirectory '__WORKDIR__' -PassThru
$app.WaitForExit()
# Hold LOGON_WITH_PROFILE open while the NSIS child installs and restarts the app.
# The administrator's finally block terminates this wrapper after verification.
$deadline = [DateTime]::UtcNow.AddMinutes(10)
while ([DateTime]::UtcNow -lt $deadline) { Start-Sleep -Seconds 1 }
'@
  $wrapperCode = $wrapperCode.Replace('__APP_EXE__', $exe.Replace("'", "''")).Replace('__WORKDIR__', $account.workdir.Replace("'", "''"))
  $wrapperPath = Join-Path $account.workdir 'launch-instrumented.ps1'
  Set-Content -LiteralPath $wrapperPath -Value $wrapperCode -Encoding utf8NoBOM
  $wrapper = Start-Process -FilePath (Get-Process -Id $PID).Path -Credential $credential -LoadUserProfile -WorkingDirectory $account.workdir -ArgumentList @('-NoProfile','-NonInteractive','-STA','-ExecutionPolicy','Bypass','-File',"`"$wrapperPath`"") -PassThru
  $ready = $false
  for ($i = 0; $i -lt 60; $i++) {
    Start-Sleep -Seconds 1
    if (-not $app) {
      $app = Get-Process -Name 'blueengine-games-launcher' -ErrorAction SilentlyContinue | Where-Object { $_.Id -notin $beforePids -and $_.Path -eq $exe } | Select-Object -First 1
      if (-not $app) {
        $wrapper.Refresh()
        if ($wrapper.HasExited) { throw "Standard-user launcher wrapper exited before app discovery: $($wrapper.ExitCode)" }
        continue
      }
    }
    $app.Refresh()
    if ($app.HasExited) { throw 'Launcher exited before opening its window' }
    if ($app.MainWindowHandle -ne 0 -and $app.MainWindowTitle -eq 'BlueEngine Games') { $ready = $true; break }
  }
  if (-not $ready) { throw 'Launcher window did not become ready' }
  if ([UpgradeToken]::IsElevated($app.Handle)) { throw 'CI launcher must run with a non-elevated token for the NSIS restart check' }
  Checkpoint "Standard-user launcher PID $($app.Id) opened its window without elevation."
  Diagnostics 'before-smoke'
  node launcher/smoke-windows.mjs
  if ($LASTEXITCODE -ne 0) { throw 'Real Windows library/native installation smoke failed' }
  Checkpoint 'Windows library smoke passed; triggering corrupt-download rejection and signed upgrade.'
  node launcher/smoke-upgrade.mjs click
  if ($LASTEXITCODE -ne 0) { throw 'Automatic update detection or user update action failed' }
  if (-not $app.WaitForExit(180000)) { throw 'Signed updater did not exit the previous launcher' }
  Checkpoint "Previous launcher PID $($app.Id) exited; waiting for the signed production process."
  Diagnostics 'after-exit'
  $restarted = $null
  $restartDeadline = [DateTime]::UtcNow.AddSeconds(180)
  while ([DateTime]::UtcNow -lt $restartDeadline) {
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
  $maximizedDeadline = [DateTime]::UtcNow.AddSeconds(10)
  $initialProductionMaximized = $false
  do {
    $restarted.Refresh()
    if ($restarted.HasExited) { throw 'Production launcher exited before its startup window state was verified' }
    if ($restarted.MainWindowHandle -ne 0 -and [UpgradeToken]::IsZoomed($restarted.MainWindowHandle)) {
      $initialProductionMaximized = $true
      break
    }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $maximizedDeadline)
  if (-not $initialProductionMaximized) { throw 'Production launcher did not start maximized' }
  Checkpoint "Production launcher PID $($restarted.Id) opened its window with the expected executable hash."
  $before = Get-Content launcher-smoke/upgrade-before.json | ConvertFrom-Json
  if ((Get-Content $before.marker -Raw) -ne "Keep player files across a signed launcher upgrade`n") { throw 'Launcher upgrade changed the player save marker' }
  $health = Invoke-RestMethod 'http://127.0.0.1:9224/health'
  if ($health.manifestRequests -lt 1 -or $health.installerRequests -ne 2) { throw 'Upgrade did not reject the corrupt download and then retry the genuine signed artifact' }
  $receipt = @{version=$version; previous_pid=$app.Id; restarted_pid=$restarted.Id; executable=$restarted.Path; sha256=$expectedHash; initial_production_maximized=$initialProductionMaximized; player_file_preserved=$true; corrupt_signature_rejected=$true; signed_feed=$health}
  $receipt | ConvertTo-Json -Depth 5 | Set-Content launcher-smoke/upgrade-result.json -Encoding utf8NoBOM
  Diagnostics 'success'
  Write-Host "Verified automatic detection, user-triggered signed update, old-process exit and production $version restart."
} catch {
  $failure = $_
  Checkpoint "Upgrade verification failed: $($_.Exception.Message)"
  Diagnostics 'failure'
  try {
    Add-Type -AssemblyName System.Windows.Forms,System.Drawing
    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bitmap = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
      $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
      $bitmap.Save((Join-Path $root 'launcher-smoke/windows-upgrade-failure.png'), [System.Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
  } catch { Write-Warning "Could not capture the Windows desktop: $_" }
  throw $failure
} finally {
  Get-Process -Name 'blueengine-games-launcher' -ErrorAction SilentlyContinue | Stop-Process -Force
  # Includes NSIS/WebView descendants after the original app PID has exited.
  Get-Process -IncludeUserName -ErrorAction SilentlyContinue | Where-Object { $_.UserName -eq $account.user } | Stop-Process -Force -ErrorAction SilentlyContinue
  if ($wrapper) {
    $wrapper.Refresh()
    if (-not $wrapper.HasExited) { Stop-Process -Id $wrapper.Id -Force }
  }
  $server.Refresh()
  if (-not $server.HasExited) { Stop-Process -Id $server.Id -Force }
}
