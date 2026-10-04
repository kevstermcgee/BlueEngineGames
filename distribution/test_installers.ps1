# Build two real installers from tiny payloads; install, upgrade and uninstall on Windows.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = Split-Path $PSScriptRoot
$temp = Join-Path ([IO.Path]::GetTempPath()) "blueengine-installer-tests-$([Guid]::NewGuid().ToString('N'))"
$name = 'BlueEngine Distribution Test'
$shortcut = Join-Path ([Environment]::GetFolderPath('Desktop')) "$name.lnk"
$install = Join-Path $temp 'installed'
$dist = Join-Path $temp 'dist'
function Assert($condition, $message) { if (-not $condition) { throw $message } }
function Run-Setup($path, $argsList) {
    $process = Start-Process -FilePath $path -ArgumentList $argsList -Wait -PassThru
    Assert ($process.ExitCode -eq 0) "Installer exited with $($process.ExitCode)"
}
try {
    New-Item -ItemType Directory (Join-Path $temp '.github/scripts'),(Join-Path $temp 'distribution'),$dist | Out-Null
    Copy-Item (Join-Path $repoRoot '.github/scripts/package_installers.ps1'),(Join-Path $repoRoot '.github/scripts/game_installer.iss') (Join-Path $temp '.github/scripts')
    Copy-Item (Join-Path $repoRoot 'distribution/GameUpdater.cs') (Join-Path $temp 'distribution')
    foreach ($version in 'one','two') {
        $stage = Join-Path $temp "stage-$version"
        New-Item -ItemType Directory $stage | Out-Null
        Copy-Item $env:ComSpec (Join-Path $stage 'Play-test-game.exe')
        Set-Content (Join-Path $stage 'version.txt') $version
        $zip = Join-Path $dist 'test-game-windows-x64.zip'
        if (Test-Path $zip) { Remove-Item $zip }
        Compress-Archive (Join-Path $stage '*') $zip
        $env:RELEASE_TAG = "test-$version"
        $header = @('slug','name','description','created','game_version','engine_version','kind','asset','release','sha256') -join "`t"
        $row = @('test-game',$name,'Test game','2026-01-01','0.1.0','engine','native','https://example.com/game.zip',$env:RELEASE_TAG,'') -join "`t"
        Set-Content (Join-Path $dist 'Games-catalog.tsv') @($header,$row)
        Set-Content (Join-Path $dist 'release-notes.md') 'Test'
        & (Join-Path $temp '.github/scripts/package_installers.ps1')
        $catalog = Get-Content -Raw (Join-Path $dist 'Games-catalog.tsv') | ConvertFrom-Csv -Delimiter "`t"
        Assert ($catalog.sha256 -eq (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()) 'ZIP checksum does not match catalog'
        Run-Setup (Join-Path $dist 'test-game-setup-windows-x64.exe') @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',"/DIR=`"$install`"",'/TASKS=desktopicon')
        Assert ((Get-Content (Join-Path $install '.installed-release')) -eq $env:RELEASE_TAG) 'Installation receipt missing'
        Assert ((Get-Content (Join-Path $install 'version.txt')) -eq $version) 'Installer did not replace payload'
        Assert (Test-Path (Join-Path $install 'Update-test-game.exe')) 'Update helper not installed'
        Assert (Test-Path $shortcut) 'Desktop shortcut was not created'
        $link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
        Assert ($link.TargetPath -eq (Join-Path $install 'Play-test-game.exe')) 'Desktop shortcut targets the wrong executable'
        if ($version -eq 'one') {
            New-Item -ItemType Directory (Join-Path $install 'saves') | Out-Null
            Set-Content (Join-Path $install 'saves/quick.be2save') 'progress'
            Set-Content (Join-Path $install 'audio-settings.json') 'settings'
        } else {
            Assert ((Get-Content (Join-Path $install 'saves/quick.be2save')) -eq 'progress') 'Upgrade lost saves'
            Assert ((Get-Content (Join-Path $install 'audio-settings.json')) -eq 'settings') 'Upgrade lost settings'
        }
    }
    $updatesLink = Join-Path ([Environment]::GetFolderPath('Programs')) "BlueEngine Games/$name - Check for updates.lnk"
    Assert (Test-Path $updatesLink) 'Start Menu update shortcut missing'
    # Simulate a later updater adding package files beyond the original Inno log.
    Set-Content (Join-Path $install 'update-added.txt') 'new package asset'
    $newHash = (Get-FileHash (Join-Path $install 'update-added.txt') -Algorithm SHA256).Hash.ToLowerInvariant()
    Add-Content (Join-Path $install '.installed-files.tsv') "update-added.txt`t$newHash"
    Run-Setup (Join-Path $install 'unins000.exe') @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART')
    Assert (Test-Path (Join-Path $install 'saves/quick.be2save')) 'Uninstaller removed player progress'
    Assert (-not (Test-Path (Join-Path $install 'Play-test-game.exe'))) 'Uninstaller left game executable'
    Assert (-not (Test-Path (Join-Path $install 'update-added.txt'))) 'Uninstaller left updater-added package files'
    Assert (-not (Test-Path $shortcut)) 'Uninstaller left desktop shortcut'
    Write-Output 'Installer tests passed: install, receipt, checksum, shortcuts, upgrade, save preservation, uninstall.'
} finally {
    if (Test-Path $temp) { Remove-Item $temp -Recurse -Force }
}
