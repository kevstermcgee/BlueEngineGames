param([switch]$Launch)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = Split-Path -Parent $PSCommandPath
$config = Get-Content -Raw -LiteralPath (Join-Path $repoRoot '.launcher-config.json') | ConvertFrom-Json
$build = Join-Path $repoRoot 'launcher\build'
& (Join-Path $repoRoot 'launcher\build.ps1') -OutputDirectory $build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$install = Join-Path $env:LOCALAPPDATA ([string]$config.install_folder)
if (Test-Path -LiteralPath $install) { Remove-Item -LiteralPath $install -Recurse -Force }
New-Item -ItemType Directory -Path $install | Out-Null
Copy-Item -Path (Join-Path $build '*') -Destination $install -Recurse
$desktop = [Environment]::GetFolderPath('Desktop')
$shortcutPath = Join-Path $desktop "$($config.product).lnk"
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$launcherExe = Join-Path $install ([string]$config.launcher_exe)
$shortcut.TargetPath = $env:ComSpec
$shortcut.Arguments = "/c start `"`" `"$launcherExe`""
$shortcut.WorkingDirectory = $install
$shortcut.IconLocation = "$launcherExe,0"
$shortcut.WindowStyle = 7
$shortcut.Description = "$($config.product) - install and play games"
$shortcut.Save()
Write-Output "Installed $($config.product) to $install"
Write-Output "Created desktop shortcut $shortcutPath"
if ($Launch) { Start-Process -FilePath $launcherExe -WorkingDirectory $install }
