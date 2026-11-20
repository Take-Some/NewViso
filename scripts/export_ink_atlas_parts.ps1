# Extract exactly named Ink texture parts from a WolvenKit-exported original PNG.
# Crops use source UV coordinates, not guessed positions. No fonts included.
param(
  [Parameter(Mandatory=$true)][string]$AtlasJson,
  [Parameter(Mandatory=$true)][string]$TexturePng,
  [Parameter(Mandatory=$true)][string]$OutputDirectory,
  [int]$Slot = 0
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$atlas = Get-Content -LiteralPath $AtlasJson -Raw -Encoding UTF8 | ConvertFrom-Json
if ($atlas.Data.RootChunk.'$type' -ne 'inkTextureAtlas') { throw 'Not a CR2W inkTextureAtlas' }
$slots = @($atlas.Data.RootChunk.slots.Elements)
if ($Slot -lt 0 -or $Slot -ge $slots.Count) { throw "Invalid slot: $Slot" }
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$bitmap = [System.Drawing.Bitmap]::FromFile($TexturePng)
$manifest = @()
try {
  $index = 0
  foreach ($part in $slots[$Slot].parts) {
    $key = [string]$part.partName.'$value'
    if ([string]::IsNullOrWhiteSpace($key)) { continue }
    $rect = $part.clippingRectInUVCoords
    $x0 = [int][Math]::Floor(([double]$rect.Left) * $bitmap.Width)
    $y0 = [int][Math]::Floor(([double]$rect.Top) * $bitmap.Height)
    $x1 = [int][Math]::Ceiling(([double]$rect.Right) * $bitmap.Width)
    $y1 = [int][Math]::Ceiling(([double]$rect.Bottom) * $bitmap.Height)
    $x0 = [Math]::Max(0,[Math]::Min($x0,$bitmap.Width - 1))
    $y0 = [Math]::Max(0,[Math]::Min($y0,$bitmap.Height - 1))
    $x1 = [Math]::Max($x0 + 1,[Math]::Min($x1,$bitmap.Width))
    $y1 = [Math]::Max($y0 + 1,[Math]::Min($y1,$bitmap.Height))
    $width = $x1 - $x0
    $height = $y1 - $y0
    if ($width -lt 1 -or $height -lt 1 -or ($width * $height) -gt 16777216) {
      throw "Invalid source atlas part size for '$key'"
    }
    $safe = [regex]::Replace($key, '[^a-zA-Z0-9_-]', '_').ToLowerInvariant()
    $filename = ('slot{0}_{1:D3}_{2}.png' -f $Slot,$index,$safe)
    $target = Join-Path $OutputDirectory $filename
    $crop = New-Object System.Drawing.Bitmap($width,$height,[System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($crop)
    try {
      $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
      $graphics.DrawImage($bitmap,
        (New-Object System.Drawing.Rectangle(0,0,$width,$height)),
        (New-Object System.Drawing.Rectangle($x0,$y0,$width,$height)),
        [System.Drawing.GraphicsUnit]::Pixel)
    } finally { $graphics.Dispose() }
    try { $crop.Save($target,[System.Drawing.Imaging.ImageFormat]::Png) }
    finally { $crop.Dispose() }
    $manifest += [ordered]@{
      name=$key
      slot=$Slot
      filename=$filename
      width=$width
      height=$height
      uv_rect=@([double]$rect.Left,[double]$rect.Top,[double]$rect.Right,[double]$rect.Bottom)
    }
    $index++
  }
} finally { $bitmap.Dispose() }
[Console]::WriteLine("INK_ATLAS_PARTS=$($manifest.Count)")
$manifestFile=Join-Path $OutputDirectory ('slot{0}_manifest.json' -f $Slot)
[IO.File]::WriteAllText($manifestFile,($manifest | ConvertTo-Json -Depth 6),
   [Text.UTF8Encoding]::new($false))
[Console]::WriteLine("MANIFEST=$manifestFile")
