# Build two real installers from tiny payloads; install, upgrade and uninstall on Windows.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = Split-Path $PSScriptRoot
$temp = Join-Path ([IO.Path]::GetTempPath()) "blueengine-installer-tests-$([Guid]::NewGuid().ToString('N'))"
$name = "BlueEngine Distribution Test $([Guid]::NewGuid().ToString('N').Substring(0, 8))"
$shortcut = Join-Path ([Environment]::GetFolderPath('DesktopDirectory', 'Create')) "$name.lnk"
$playLink = Join-Path ([Environment]::GetFolderPath('Programs')) "BlueEngine Games/$name.lnk"
$updatesLink = Join-Path ([Environment]::GetFolderPath('Programs')) "BlueEngine Games/$name - Check for updates.lnk"
$desktopPathOwned = $false
$markerHash = ''
$install = Join-Path $temp 'installed'
$dist = Join-Path $temp 'dist'
function Assert($condition, $message) { if (-not $condition) { throw $message } }
function Run-Setup($path, $argsList) {
    $process = Start-Process -FilePath $path -ArgumentList $argsList -Wait -PassThru
    Assert ($process.ExitCode -eq 0) "Installer exited with $($process.ExitCode)"
}
try {
    Assert (-not (Test-Path $shortcut)) 'The fixture desktop path already exists; refusing to touch it'
    $desktopPathOwned = $true
    New-Item -ItemType Directory (Join-Path $temp '.github/scripts'),(Join-Path $temp 'distribution'),$dist | Out-Null
    Copy-Item (Join-Path $repoRoot '.github/scripts/package_installers.ps1'),(Join-Path $repoRoot '.github/scripts/game_installer.iss') (Join-Path $temp '.github/scripts')
    foreach ($file in 'GameUpdater.cs','game_metadata.py','package_metadata.py') {
        Copy-Item (Join-Path $repoRoot "distribution/$file") (Join-Path $temp 'distribution')
    }
    foreach ($version in 'one','two') {
        $stage = Join-Path $temp "stage-$version"
        New-Item -ItemType Directory $stage | Out-Null
        Copy-Item $env:ComSpec (Join-Path $stage 'Play-test-game.exe')
        $originalExeHash = (Get-FileHash (Join-Path $stage 'Play-test-game.exe') -Algorithm SHA256).Hash
        # ComSpec is an immutable signed fixture. Describe it through our ZIP and
        # fresh Inno installer, without appending anything to the signed binary.
        $fixtureWriter = @'
import sys
from pathlib import Path
sys.path.insert(0, sys.argv[3])
from game_metadata import canonical_payload
document = {'format':'blueengine.game','schema_version':1,
 'game':{'slug':'test-game','title':sys.argv[2],'version':'0.1.0','description':'Installer compatibility fixture','controls':'','authors':[],'tags':[]},
 'source':None,'engine':None,'posting':{'source_status':'unavailable'}}
Path(sys.argv[1]).write_bytes(canonical_payload(document))
'@
        & python -c $fixtureWriter (Join-Path $stage 'blueengine.game.json') $name (Join-Path $temp 'distribution')
        Assert ($LASTEXITCODE -eq 0) 'Could not create fixture metadata'
        Set-Content (Join-Path $stage 'version.txt') $version
        Add-Type -AssemblyName System.Drawing
        $iconFile = [IO.File]::Create((Join-Path $stage 'game.ico'))
        try { [Drawing.SystemIcons]::Application.Save($iconFile) } finally { $iconFile.Dispose() }
        $zip = Join-Path $dist 'test-game-windows-x64.zip'
        if (Test-Path $zip) { Remove-Item $zip }
        Compress-Archive (Join-Path $stage '*') $zip
        & python (Join-Path $temp 'distribution/package_metadata.py') archive --stage $stage --archive $zip
        Assert ($LASTEXITCODE -eq 0) 'Could not embed fixture ZIP metadata'
        $env:RELEASE_TAG = "test-$version"
        $header = @('slug','name','description','created','game_version','engine_version','kind','asset','release','sha256') -join "`t"
        $row = @('test-game',$name,'Test game','2026-01-01','0.1.0','engine','native','https://example.com/game.zip',$env:RELEASE_TAG,'') -join "`t"
        Set-Content (Join-Path $dist 'Games-catalog.tsv') @($header,$row)
        Set-Content (Join-Path $dist 'release-notes.md') 'Test'
        $originalHash = (Get-FileHash $zip -Algorithm SHA256).Hash
        & (Join-Path $temp '.github/scripts/package_installers.ps1') -PreserveArchives:($version -eq 'two')
        if ($version -eq 'two') { Assert ((Get-FileHash $zip -Algorithm SHA256).Hash -eq $originalHash) 'Archive packaging changed the original ZIP' }
        $catalog = Get-Content -Raw (Join-Path $dist 'Games-catalog.tsv') | ConvertFrom-Csv -Delimiter "`t"
        Assert ($catalog.sha256 -eq (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()) 'ZIP checksum does not match catalog'
        $zipMetadata = & python (Join-Path $temp 'distribution/game_metadata.py') inspect $zip | ConvertFrom-Json
        Assert ($LASTEXITCODE -eq 0 -and $zipMetadata.game.slug -eq 'test-game') 'Updater ZIP lost portable metadata'
        $setup = Join-Path $dist 'test-game-setup-windows-x64.exe'
        $setupMetadata = & python (Join-Path $temp 'distribution/game_metadata.py') inspect $setup | ConvertFrom-Json
        Assert ($LASTEXITCODE -eq 0 -and $setupMetadata.game.title -eq $name) 'Fresh Inno installer lost portable metadata'
        Assert (($setupMetadata | ConvertTo-Json -Depth 20 -Compress) -eq ($zipMetadata | ConvertTo-Json -Depth 20 -Compress)) 'ZIP and installer metadata disagree'
        $setupHash = (Get-FileHash $setup -Algorithm SHA256).Hash.ToLowerInvariant()
        Assert ((Get-Content (Join-Path $dist 'SHA256SUMS.txt')) -contains "$setupHash  test-game-setup-windows-x64.exe") 'Installer checksum predates metadata embedding'
        Run-Setup (Join-Path $dist 'test-game-setup-windows-x64.exe') @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',"/DIR=`"$install`"",'/TASKS=desktopicon')
        Assert ((Get-Content (Join-Path $install '.installed-release')) -eq $env:RELEASE_TAG) 'Installation receipt missing'
        Assert ((Get-Content (Join-Path $install 'version.txt')) -eq $version) 'Installer did not replace payload'
        Assert ((Get-FileHash (Join-Path $install 'Play-test-game.exe') -Algorithm SHA256).Hash -eq $originalExeHash) 'Packaging modified the immutable signed fixture'
        Assert ((Get-Content -Raw (Join-Path $install 'blueengine.game.json') | ConvertFrom-Json).game.slug -eq 'test-game') 'Installed game lost portable metadata'
        Assert (Test-Path (Join-Path $install 'Update-test-game.exe')) 'Update helper not installed'
        # Even a remembered legacy task request must not create a game desktop shortcut.
        Assert (Test-Path $playLink) 'Start Menu game shortcut missing'
        $link = (New-Object -ComObject WScript.Shell).CreateShortcut($playLink)
        Assert ($link.TargetPath -eq (Join-Path $install 'Play-test-game.exe')) 'Start Menu shortcut targets the wrong executable'
        Assert (Test-Path $updatesLink) 'Start Menu update shortcut missing'
        if ($version -eq 'one') {
            Assert (-not (Test-Path $shortcut)) 'Installer created an unwanted desktop shortcut'
            # A player's item at the former desktop path must remain player-owned.
            Set-Content -LiteralPath $shortcut -Value 'User-created desktop marker; preserve during upgrade and uninstall.' -Encoding utf8
            $markerHash = (Get-FileHash $shortcut -Algorithm SHA256).Hash
            New-Item -ItemType Directory (Join-Path $install 'saves') | Out-Null
            Set-Content (Join-Path $install 'saves/quick.be2save') 'progress'
            Set-Content (Join-Path $install 'audio-settings.json') 'settings'
        } else {
            Assert (Test-Path $shortcut) 'Upgrade removed a user-created desktop item'
            Assert ((Get-FileHash $shortcut -Algorithm SHA256).Hash -eq $markerHash) 'Upgrade replaced a user-created desktop item'
            Assert ((Get-Content (Join-Path $install 'saves/quick.be2save')) -eq 'progress') 'Upgrade lost saves'
            Assert ((Get-Content (Join-Path $install 'audio-settings.json')) -eq 'settings') 'Upgrade lost settings'
        }
    }
    # Simulate a later updater adding package files beyond the original Inno log.
    Set-Content (Join-Path $install 'update-added.txt') 'new package asset'
    $newHash = (Get-FileHash (Join-Path $install 'update-added.txt') -Algorithm SHA256).Hash.ToLowerInvariant()
    Add-Content (Join-Path $install '.installed-files.tsv') "update-added.txt`t$newHash"
    Run-Setup (Join-Path $install 'unins000.exe') @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART')
    Assert (Test-Path (Join-Path $install 'saves/quick.be2save')) 'Uninstaller removed player progress'
    Assert (-not (Test-Path (Join-Path $install 'Play-test-game.exe'))) 'Uninstaller left game executable'
    Assert (-not (Test-Path (Join-Path $install 'update-added.txt'))) 'Uninstaller left updater-added package files'
    Assert (-not (Test-Path $playLink)) 'Uninstaller left the Start Menu game shortcut'
    Assert (-not (Test-Path $updatesLink)) 'Uninstaller left the Start Menu update shortcut'
    Assert (Test-Path $shortcut) 'Uninstaller removed a user-created desktop item'
    Assert ((Get-FileHash $shortcut -Algorithm SHA256).Hash -eq $markerHash) 'Uninstaller changed a user-created desktop item'
    Write-Output 'Installer tests passed: portable ZIP and Inno metadata, final checksum, signed payload preservation, no automatic desktop shortcut, player desktop item preservation, Start Menu shortcuts, receipt, upgrade, saves and uninstall.'
} finally {
    if ($desktopPathOwned -and (Test-Path $shortcut)) { Remove-Item -LiteralPath $shortcut -Force }
    if (Test-Path $temp) { Remove-Item $temp -Recurse -Force }
}
