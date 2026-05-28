$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
$docs = Join-Path $PSScriptRoot "..\docs"
New-Item -ItemType Directory -Force -Path $docs | Out-Null
foreach ($name in @("screenshot-library.png", "screenshot-search.png")) {
  $path = Join-Path $docs $name
  $bitmap = New-Object System.Drawing.Bitmap 800, 500
  $bitmap.MakeTransparent()
  $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $bitmap.Dispose()
}
Write-Host "Placeholder screenshots written to $docs"
