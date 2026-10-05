$ErrorActionPreference = 'Stop'
$csc = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
$out = Join-Path ([IO.Path]::GetTempPath()) "blueengine-updater-tests-$([Guid]::NewGuid().ToString('N')).exe"
try {
    & $csc /nologo /target:exe /main:UpdaterTests "/out:$out" /reference:System.dll /reference:System.Core.dll /reference:System.Drawing.dll /reference:System.Windows.Forms.dll /reference:System.IO.Compression.dll /reference:System.IO.Compression.FileSystem.dll (Join-Path $PSScriptRoot 'GameUpdater.cs') (Join-Path $PSScriptRoot 'UpdaterTests.cs')
    if ($LASTEXITCODE -ne 0) { throw 'Updater tests failed to compile.' }
    & $out
    if ($LASTEXITCODE -ne 0) { throw 'Updater tests failed.' }
} finally { if (Test-Path $out) { Remove-Item $out } }
