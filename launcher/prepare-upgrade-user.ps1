# Ephemeral Windows CI account. Its DPAPI-protected password is never an artifact.
$ErrorActionPreference = 'Stop'
$name = 'BlueEngineCI'
if (Get-LocalUser -Name $name -ErrorAction SilentlyContinue) { throw 'Reserved launcher CI account already exists' }
$password = ConvertTo-SecureString ('Aa1!' + [Convert]::ToBase64String([System.Security.Cryptography.RandomNumberGenerator]::GetBytes(32))) -AsPlainText -Force
$user = New-LocalUser -Name $name -Password $password -PasswordNeverExpires -UserMayNotChangePassword
if (-not (Get-LocalGroupMember -Group 'Users' | Where-Object { $_.SID -eq $user.SID })) { Add-LocalGroupMember -Group 'Users' -Member $name }
if (Get-LocalGroupMember -Group 'Administrators' | Where-Object { $_.SID -eq $user.SID }) { throw 'Launcher CI account must never be an administrator' }
$credential = [PSCredential]::new("$env:COMPUTERNAME\$name", $password)
$credential | Export-Clixml (Join-Path $env:RUNNER_TEMP 'blueengine-upgrade-credential.xml')
$workdir = Join-Path $env:RUNNER_TEMP 'blueengine-upgrade-user'
New-Item -ItemType Directory -Path $workdir -Force | Out-Null
icacls $workdir /grant "*$($user.SID.Value):(OI)(CI)M" | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not grant the CI account access to its isolated work directory' }
$profileFile = Join-Path $workdir 'profile.json'
$probe = @'
$ErrorActionPreference = 'Stop'
$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$sid = $identity.User.Value
$profile = [Environment]::ExpandEnvironmentVariables((Get-ItemProperty -LiteralPath "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\$sid").ProfileImagePath)
$env:USERPROFILE = $profile
$env:LOCALAPPDATA = Join-Path $profile 'AppData\Local'
$env:APPDATA = Join-Path $profile 'AppData\Roaming'
@{sid=$sid; user=$identity.Name; profile=$profile; local_app_data=$env:LOCALAPPDATA; app_data=$env:APPDATA} | ConvertTo-Json | Set-Content -LiteralPath '__PROFILE_FILE__' -Encoding utf8
Add-Type -AssemblyName System.Windows.Forms
$form = [System.Windows.Forms.Form]::new()
$form.Text = 'BlueEngine updater preflight'
$form.Width = 400
$form.Height = 150
$timer = [System.Windows.Forms.Timer]::new()
$timer.Interval = 45000
$timer.Add_Tick({ $form.Close() })
$timer.Start()
[System.Windows.Forms.Application]::Run($form)
'@
$probe = $probe.Replace('__PROFILE_FILE__', $profileFile.Replace("'", "''"))
$probePath = Join-Path $workdir 'desktop-preflight.ps1'
Set-Content -LiteralPath $probePath -Value $probe -Encoding utf8NoBOM
$shell = (Get-Process -Id $PID).Path
$process = Start-Process -FilePath $shell -Credential $credential -LoadUserProfile -WorkingDirectory $workdir -ArgumentList @('-NoProfile','-NonInteractive','-STA','-ExecutionPolicy','Bypass','-File',"`"$probePath`"") -PassThru
try {
  Add-Type @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class UpgradePreflightToken {
  [DllImport("advapi32.dll", SetLastError=true)] static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
  [DllImport("advapi32.dll", SetLastError=true)] static extern bool GetTokenInformation(IntPtr token, int kind, out int value, int size, out int returned);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
  public static bool IsElevated(IntPtr process) {
    IntPtr token;
    if (!OpenProcessToken(process, 8, out token)) throw new Win32Exception();
    try { int value, returned; if (!GetTokenInformation(token, 20, out value, 4, out returned)) throw new Win32Exception(); return value != 0; }
    finally { CloseHandle(token); }
  }
}
'@
  $ready = $false
  for ($i = 0; $i -lt 30; $i++) {
    Start-Sleep -Seconds 1
    $process.Refresh()
    if ($process.HasExited) { throw "Standard-user desktop preflight exited with code $($process.ExitCode)" }
    if ($process.MainWindowHandle -ne 0 -and $process.MainWindowTitle -eq 'BlueEngine updater preflight' -and (Test-Path $profileFile)) { $ready = $true; break }
  }
  if (-not $ready) { throw 'Standard-user desktop preflight did not open its window' }
  if ([UpgradePreflightToken]::IsElevated($process.Handle)) { throw 'Dedicated standard-user process is unexpectedly elevated' }
  $profileInfo = Get-Content -LiteralPath $profileFile | ConvertFrom-Json
  if ($profileInfo.sid -ne $user.SID.Value) { throw 'Desktop preflight used the wrong Windows identity' }
  $profileInfo | Add-Member -NotePropertyName workdir -NotePropertyValue $workdir
  $profileInfo | ConvertTo-Json | Set-Content (Join-Path $env:RUNNER_TEMP 'blueengine-upgrade-user.json') -Encoding utf8NoBOM
  Write-Host "Standard-user desktop preflight passed: $($profileInfo.user), TokenElevation=false, visible window, isolated profile."
} finally {
  $process.Refresh()
  if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
}
