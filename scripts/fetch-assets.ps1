# Downloads the mouse image and the font used by the UI from the ATK Hub CDN.
# These files belong to their owners and are not in the repository: everyone downloads them locally.
$ErrorActionPreference = 'Stop'
$assets = Join-Path $PSScriptRoot '..\ui\assets'
$tmp = Join-Path ([IO.Path]::GetTempPath()) 'a9-panel-assets'
New-Item -ItemType Directory -Force $assets, $tmp | Out-Null

# pink A9 Ultimate image (WebP -> PNG, keeping transparency)
$webp = Join-Path $tmp 'mouse.webp'
Invoke-WebRequest 'https://bpcdn.atkgear.com/device/ossAssets/a9nearlink/rose-red.webp' -OutFile $webp -UseBasicParsing
Add-Type -AssemblyName PresentationCore
$in = [IO.File]::OpenRead($webp)
try {
    $frame = [Windows.Media.Imaging.BitmapDecoder]::Create($in, 'PreservePixelFormat', 'OnLoad').Frames[0]
    $bgra = New-Object Windows.Media.Imaging.FormatConvertedBitmap($frame, [Windows.Media.PixelFormats]::Bgra32, $null, 0)
    $enc = New-Object Windows.Media.Imaging.PngBitmapEncoder
    $enc.Frames.Add([Windows.Media.Imaging.BitmapFrame]::Create($bgra))
    $out = [IO.File]::Create((Join-Path $assets 'mouse.png'))
    try { $enc.Save($out) } finally { $out.Close() }
} finally { $in.Close() }

# OPPO Sans 4.0 font (WOFF -> TTF, Slint only reads TTF/OTF)
$woff = Join-Path $tmp 'oppo.woff'
Invoke-WebRequest 'https://bpcdn.atkgear.com/hub-v3/production/3.2.27/static/OPPO-Sans-4.0-CLWyZFQr.woff' -OutFile $woff -UseBasicParsing
python -I (Join-Path $PSScriptRoot 'woff2ttf.py') $woff (Join-Path $assets 'OPPOSans.ttf')
if ($LASTEXITCODE -ne 0) { throw 'Font conversion failed (needs Python 3)' }

Write-Host 'Done: ui/assets/mouse.png and ui/assets/OPPOSans.ttf'
