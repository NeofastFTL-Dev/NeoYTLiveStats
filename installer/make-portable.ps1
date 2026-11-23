# PORTABLE-V1
# Packs the single-file publish output (plus the README) into a portable ZIP that needs no
# installation: unpack anywhere and run YTLiveStats.exe. Settings and the source health log still
# live in %APPDATA%\YTLiveStats, so the portable build and the installed build share them.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File installer\make-portable.ps1 -Version 1.2.3
#
# Writes dist\YTLiveStats-<version>-portable.zip next to the setup exe. The version comes from the
# build (the v* tag in CI), so nothing here is versioned by hand; -OutFile overrides the name.

param(
  [string]$Version,
  [string]$Payload = (Join-Path $PSScriptRoot '..\dist\publish'),
  [string]$OutFile
)

$ErrorActionPreference = 'Stop'

if (-not $OutFile) {
  if (-not $Version) {
    throw 'specify -Version <x.y.z> (or -OutFile): the ZIP is named after the version'
  }
  $OutFile = Join-Path $PSScriptRoot "..\dist\YTLiveStats-$Version-portable.zip"
}

$payloadExe = Join-Path $Payload 'YTLiveStats.exe'
if (-not (Test-Path -LiteralPath $payloadExe)) {
  throw "publish output missing: $payloadExe (run dotnet publish first)"
}

$readme = Join-Path $PSScriptRoot '..\README.md'
$staging = Join-Path ([System.IO.Path]::GetTempPath()) ('ytl-portable-' + [guid]::NewGuid().ToString('N'))

New-Item -ItemType Directory -Path $staging | Out-Null
try {
  Copy-Item -LiteralPath $payloadExe -Destination (Join-Path $staging 'YTLiveStats.exe')
  if (Test-Path -LiteralPath $readme) {
    Copy-Item -LiteralPath $readme -Destination (Join-Path $staging 'README.md')
  }

  $target = [System.IO.Path]::GetFullPath($OutFile)
  if (Test-Path -LiteralPath $target) {
    Remove-Item -LiteralPath $target -Force
  }

  Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $target -CompressionLevel Optimal

  $zip = Get-Item -LiteralPath $target
  $entries = ([System.IO.Compression.ZipFile]::OpenRead($target)).Entries.Count
  "wrote {0} ({1} bytes, {2} entries)" -f $zip.FullName, $zip.Length, $entries
}
finally {
  Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
}
