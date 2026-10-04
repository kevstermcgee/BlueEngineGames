param(
    [int]$ReleaseLimit = 5,
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '../../dist/backfill'),
    [switch]$Publish
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$pages = & gh api --paginate --slurp repos/kevstermcgee/BlueEngineGames/releases | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Could not list historical releases.' }
$releases = @($pages | ForEach-Object { $_ } | Where-Object { -not $_.draft -and -not $_.prerelease } | Sort-Object published_at -Descending)
if ($ReleaseLimit -gt 0) { $releases = @($releases | Select-Object -First $ReleaseLimit) }
New-Item -ItemType Directory $OutputDirectory -Force | Out-Null
foreach ($release in $releases) {
    $tag = [string]$release.tag_name
    if ($tag -notmatch '^[a-zA-Z0-9._-]+$') { throw "Unsafe release tag: $tag" }
    $missing = @($release.assets | Where-Object {
        $_.name -cmatch '^[a-z0-9]+(?:-[a-z0-9]+)*-windows-x64\.zip$' -and
        ($release.assets.name -notcontains ($_.name -replace '-windows-x64\.zip$', '-setup-windows-x64.exe'))
    })
    if ($missing.Count -eq 0) { continue }
    $work = Join-Path ([IO.Path]::GetTempPath()) "blueengine-archive-$([Guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory $work | Out-Null
    try {
        $metadata = @()
        $oldCatalog = @($release.assets | Where-Object name -eq 'BlueEngineLauncher-catalog.tsv')
        if ($oldCatalog.Count) {
            & gh release download $tag --repo kevstermcgee/BlueEngineGames --pattern 'BlueEngineLauncher-catalog.tsv' --dir $work
            if ($LASTEXITCODE -ne 0) { throw "Could not download catalog for $tag" }
            $metadata = @(Get-Content -Raw (Join-Path $work 'BlueEngineLauncher-catalog.tsv') | ConvertFrom-Csv -Delimiter "`t")
        }
        $fields = @('slug','name','description','created','game_version','engine_version','kind','asset','release','sha256')
        $lines = @($fields -join "`t")
        foreach ($asset in $missing) {
            & gh release download $tag --repo kevstermcgee/BlueEngineGames --pattern $asset.name --dir $work
            if ($LASTEXITCODE -ne 0) { throw "Could not download $($asset.name) from $tag" }
            $slug = $asset.name -replace '-windows-x64\.zip$', ''
            $row = @($metadata | Where-Object slug -eq $slug)
            $name = if ($row.Count) { $row[0].name } else { (Get-Culture).TextInfo.ToTitleCase($slug.Replace('-', ' ')) }
            $description = if ($row.Count) { $row[0].description } else { 'Archived BlueEngine game.' }
            $version = if ($row.Count) { $row[0].game_version } else { 'archive' }
            $hash = (Get-FileHash (Join-Path $work $asset.name) -Algorithm SHA256).Hash.ToLowerInvariant()
            $lines += (@($slug,$name,$description,([string]$release.published_at).Substring(0,10),$version,$tag,'archive',$asset.browser_download_url,$tag,$hash) -join "`t")
        }
        Set-Content (Join-Path $work 'Games-catalog.tsv') $lines -Encoding utf8NoBOM
        Set-Content (Join-Path $work 'release-notes.md') 'Archived builds packaged as Windows installers.'
        $env:RELEASE_TAG = $tag
        # Keep the old ZIP bytes intact. Only installers are added to the archive.
        & (Join-Path $PSScriptRoot 'package_installers.ps1') -DistDirectory $work -PreserveArchives
        $destination = Join-Path $OutputDirectory $tag
        New-Item -ItemType Directory $destination -Force | Out-Null
        $installers = @(Get-ChildItem $work -Filter '*-setup-windows-x64.exe')
        Copy-Item $installers.FullName $destination
        $sums = @($installers | Sort-Object Name | ForEach-Object { "$((Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $($_.Name)" })
        Set-Content (Join-Path $destination 'INSTALLER-SHA256SUMS.txt') $sums -Encoding ascii
        if ($Publish) {
            & gh release upload $tag @($installers.FullName) (Join-Path $destination 'INSTALLER-SHA256SUMS.txt') --repo kevstermcgee/BlueEngineGames
            if ($LASTEXITCODE -ne 0) { throw "Could not add archived installers to $tag" }
        }
        Write-Output "Packaged $($installers.Count) archived installers for $tag."
    } finally { Remove-Item $work -Recurse -Force }
}
