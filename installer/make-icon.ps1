# Generates app.ico: a dark rounded tile with three ascending bars (the tallest in YouTube red).
# Each size is drawn natively instead of downscaling one bitmap, and every size is stored as a
# classic 32-bit DIB entry (not PNG), so System.Drawing, the Windows shell and NSIS all read it.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File installer\make-icon.ps1
#
# Writes app.ico next to the project file plus a preview PNG to look at.

# SERIALIZE=DIB32

param(
  [string]$OutFile = (Join-Path $PSScriptRoot '..\app.ico'),
  [string]$PreviewFile = (Join-Path $PSScriptRoot '..\app-preview.png')
)

Add-Type -AssemblyName System.Drawing
$ErrorActionPreference = 'Stop'

function New-RoundedRectPath {
  param([single]$X, [single]$Y, [single]$W, [single]$H, [single]$Radius)
  $p = New-Object System.Drawing.Drawing2D.GraphicsPath
  $d = [single]([Math]::Min($Radius, [Math]::Min($W, $H) / 2) * 2)
  if ($d -le 0.5) {
    $p.AddRectangle((New-Object System.Drawing.RectangleF($X, $Y, $W, $H)))
    return $p
  }
  $p.AddArc($X, $Y, $d, $d, 180, 90)
  $p.AddArc($X + $W - $d, $Y, $d, $d, 270, 90)
  $p.AddArc($X + $W - $d, $Y + $H - $d, $d, $d, 0, 90)
  $p.AddArc($X, $Y + $H - $d, $d, $d, 90, 90)
  $p.CloseFigure()
  return $p
}

function New-IconBitmap {
  param([int]$Size)

  $s = [single]$Size
  $bmp = New-Object System.Drawing.Bitmap($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.Clear([System.Drawing.Color]::Transparent)

  # Dark rounded tile.
  $tile = New-RoundedRectPath -X 0.5 -Y 0.5 -W ($s - 1) -H ($s - 1) -Radius ($s * 0.20)
  $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.RectangleF(0, 0, $s, $s)),
    [System.Drawing.Color]::FromArgb(255, 38, 38, 47),
    [System.Drawing.Color]::FromArgb(255, 18, 18, 24),
    90.0)
  $g.FillPath($brush, $tile)
  $brush.Dispose()

  if ($Size -ge 32) {
    $edge = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(64, 255, 255, 255), [single]([Math]::Max(1.0, $s * 0.012)))
    $g.DrawPath($edge, $tile)
    $edge.Dispose()
  }

  # Three ascending bars; the tallest is the accent colour.
  $barW = $s * 0.150
  $gap = $s * 0.078
  $total = $barW * 3 + $gap * 2
  $x = ($s - $total) / 2
  $baseline = $s * 0.775
  $heights = @(0.300, 0.470, 0.645)
  $colors = @(
    [System.Drawing.Color]::FromArgb(255, 122, 122, 138),
    [System.Drawing.Color]::FromArgb(255, 201, 201, 214),
    [System.Drawing.Color]::FromArgb(255, 255, 0, 51)
  )

  for ($i = 0; $i -lt 3; $i++) {
    $h = $s * $heights[$i]
    $bar = New-RoundedRectPath -X ([single]$x) -Y ([single]($baseline - $h)) -W ([single]$barW) -H ([single]$h) -Radius ([single]($barW * 0.34))
    $fill = New-Object System.Drawing.SolidBrush($colors[$i])
    $g.FillPath($fill, $bar)
    $fill.Dispose()
    $bar.Dispose()
    $x += $barW + $gap
  }

  $tile.Dispose()
  $g.Dispose()
  return $bmp
}

# One 32-bit DIB icon entry: BITMAPINFOHEADER + bottom-up BGRA rows + a zeroed AND mask.
function Get-IconEntry {
  param([System.Drawing.Bitmap]$Bitmap)

  $w = $Bitmap.Width
  $h = $Bitmap.Height
  $rect = New-Object System.Drawing.Rectangle(0, 0, $w, $h)
  $locked = $Bitmap.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $stride = $locked.Stride
  $pixels = [byte[]]::new($stride * $h)
  [System.Runtime.InteropServices.Marshal]::Copy($locked.Scan0, $pixels, 0, $pixels.Length)
  $Bitmap.UnlockBits($locked)

  $maskRowBytes = [int]([Math]::Ceiling($w / 32.0) * 4)
  $xorSize = $w * $h * 4
  $andSize = $maskRowBytes * $h

  $ms = New-Object System.IO.MemoryStream
  $bw = New-Object System.IO.BinaryWriter($ms)
  $bw.Write([uint32]40)                  # biSize
  $bw.Write([int32]$w)                   # biWidth
  $bw.Write([int32]($h * 2))             # biHeight (XOR + AND)
  $bw.Write([uint16]1)                   # biPlanes
  $bw.Write([uint16]32)                  # biBitCount
  $bw.Write([uint32]0)                   # biCompression = BI_RGB
  $bw.Write([uint32]($xorSize + $andSize))
  $bw.Write([int32]0)                    # biXPelsPerMeter
  $bw.Write([int32]0)                    # biYPelsPerMeter
  $bw.Write([uint32]0)                   # biClrUsed
  $bw.Write([uint32]0)                   # biClrImportant

  for ($y = $h - 1; $y -ge 0; $y--) {
    $bw.Write($pixels, $y * $stride, $w * 4)
  }

  $bw.Write([byte[]]::new($andSize), 0, $andSize)
  $bw.Flush()

  $bytes = $ms.ToArray()
  $bw.Dispose()
  $ms.Dispose()
  return $bytes
}

$sizes = @(16, 24, 32, 48, 64, 128, 256)
$entries = @()

foreach ($size in $sizes) {
  $bmp = New-IconBitmap -Size $size
  if ($size -eq 256) {
    $bmp.Save($PreviewFile, [System.Drawing.Imaging.ImageFormat]::Png)
  }
  $entries += , @($size, (Get-IconEntry -Bitmap $bmp))
  $bmp.Dispose()
}

$out = New-Object System.IO.MemoryStream
$bw = New-Object System.IO.BinaryWriter($out)

$bw.Write([uint16]0)              # reserved
$bw.Write([uint16]1)              # type: icon
$bw.Write([uint16]$entries.Count) # image count

$offset = 6 + (16 * $entries.Count)
foreach ($entry in $entries) {
  $size = [int]$entry[0]
  $bytes = [byte[]]$entry[1]
  $bw.Write([byte]($(if ($size -ge 256) { 0 } else { $size })))
  $bw.Write([byte]($(if ($size -ge 256) { 0 } else { $size })))
  $bw.Write([byte]0)    # palette colours
  $bw.Write([byte]0)    # reserved
  $bw.Write([uint16]1)  # colour planes
  $bw.Write([uint16]32) # bits per pixel
  $bw.Write([uint32]$bytes.Length)
  $bw.Write([uint32]$offset)
  $offset += $bytes.Length
}

foreach ($entry in $entries) { $bw.Write([byte[]]$entry[1]) }

$bw.Flush()
$target = [System.IO.Path]::GetFullPath($OutFile)
[System.IO.File]::WriteAllBytes($target, $out.ToArray())
$bw.Dispose()
$out.Dispose()

$ico = Get-Item -LiteralPath $target
"wrote {0} ({1} bytes, {2} sizes, DIB)" -f $ico.FullName, $ico.Length, $entries.Count
