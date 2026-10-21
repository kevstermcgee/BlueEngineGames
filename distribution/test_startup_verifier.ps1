$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
# Load just the pure readiness helpers; never start a game or touch the desktop.
$tokens = $null
$errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'check_startup_windows.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($name in @('Get-StudioTitleSamples', 'Wait-StudioTitleSamples')) {
  $node = $ast.Find({ param($entry) $entry -is [Management.Automation.Language.FunctionDefinitionAst] -and $entry.Name -eq $name }, $true)
  if (-not $node) { throw "Missing startup helper $name" }
  Invoke-Expression $node.Extent.Text
}

function New-TitleFrame([Drawing.Color]$Color) {
  $bitmap = [Drawing.Bitmap]::new(900, 600)
  $graphics = [Drawing.Graphics]::FromImage($bitmap)
  $font = [Drawing.Font]::new('Segoe UI', 32)
  $brush = [Drawing.SolidBrush]::new($Color)
  try {
    $graphics.Clear([Drawing.Color]::FromArgb(5, 8, 15))
    $size = $graphics.MeasureString('BlueEngine Games', $font)
    $graphics.DrawString('BlueEngine Games', $font, $brush, [single]((900-$size.Width)/2), [single]272)
  } finally { $brush.Dispose(); $font.Dispose(); $graphics.Dispose() }
  return $bitmap
}

$dark = New-TitleFrame ([Drawing.Color]::FromArgb(125, 130, 140))
$bright = New-TitleFrame ([Drawing.Color]::FromArgb(240, 247, 255))
try {
  if ((Get-StudioTitleSamples $dark) -ne 0) { throw 'A still-fading title must not satisfy the existing brightness threshold.' }
  if ((Get-StudioTitleSamples $bright) -lt 30) { throw 'An opaque centered title must satisfy the existing threshold.' }
  $frames = [Collections.Generic.Queue[Drawing.Bitmap]]::new()
  $frames.Enqueue($dark); $frames.Enqueue($dark); $frames.Enqueue($bright)
  $result = Wait-StudioTitleSamples -Probe { Get-StudioTitleSamples ($frames.Dequeue()) }
  if ($result -lt 30 -or $frames.Count -ne 0) { throw 'Readiness must retry faded frames and accept the actual opaque frame.' }
  $clock = [Diagnostics.Stopwatch]::StartNew()
  $result = Wait-StudioTitleSamples -Probe { Get-StudioTitleSamples $dark } -TimeoutMilliseconds 120
  if ($result -ge 30 -or $clock.ElapsedMilliseconds -gt 1500) { throw 'A missing/transparent title must fail in bounded time.' }
  $result = Wait-StudioTitleSamples -Probe { 29 } -TimeoutMilliseconds 0
  if ($result -ne 29) { throw 'Timeout must not lower the title threshold or fabricate a pass.' }
} finally { $dark.Dispose(); $bright.Dispose() }
Write-Host 'Startup raster readiness: faded frames retry, opaque titles pass, dark titles remain failures.'
