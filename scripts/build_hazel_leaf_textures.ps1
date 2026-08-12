param(
    [string]$Source = "assets/textures/shrubs/source/common_hazel_leaf_plate.png",
    [string]$OutputDirectory = "assets/textures/shrubs",
    [int]$Size = 512
)

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Drawing
$sourceImage = [System.Drawing.Bitmap]::new((Resolve-Path $Source).Path)
$resized = [System.Drawing.Bitmap]::new($Size, $Size)
$graphics = [System.Drawing.Graphics]::FromImage($resized)
$graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$graphics.DrawImage($sourceImage, 0, 0, $Size, $Size)
$graphics.Dispose()
$sourceImage.Dispose()

New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$opacity = [System.Drawing.Bitmap]::new($Size, $Size)
$albedo = [System.Drawing.Bitmap]::new($Size, $Size)
$height = [System.Drawing.Bitmap]::new($Size, $Size)
$values = New-Object 'double[,]' $Size, $Size
$mask = New-Object 'bool[,]' $Size, $Size

for ($y = 0; $y -lt $Size; $y++) {
    for ($x = 0; $x -lt $Size; $x++) {
        $pixel = $resized.GetPixel($x, $y)
        $isBackground = $pixel.R -gt 205 -and $pixel.B -gt 150 -and $pixel.G -lt 90
        $inside = -not $isBackground
        $mask[$x, $y] = $inside
        $alpha = if ($inside) { 255 } else { 0 }
        $opacity.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(255, $alpha, $alpha, $alpha))
        if ($inside) {
            # Remove the residual magenta spill and compress photographed
            # illumination. This is a stable albedo transform, not a second
            # generated image, so every PBR channel remains registered.
            $red = [Math]::Min(255, [Math]::Max(0, ($pixel.R - 18) * 0.92))
            $green = [Math]::Min(255, [Math]::Max(0, ($pixel.G + 4) * 0.91))
            $blue = [Math]::Min(255, [Math]::Max(0, ($pixel.B - 12) * 0.76))
            $albedo.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(255, $red, $green, $blue))
            $luma = (0.2126 * $red + 0.7152 * $green + 0.0722 * $blue) / 255.0
            $values[$x, $y] = $luma
        } else {
            # Premultiplied-safe green dilation colour beneath transparent texels.
            $albedo.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(255, 50, 105, 31))
            $values[$x, $y] = 0.5
        }
    }
}

# A broad blade camber and photograph-derived high-frequency vein relief share
# one scalar field. The alpha edge is never interpreted as height.
$heightValues = New-Object 'double[,]' $Size, $Size
for ($y = 1; $y -lt $Size - 1; $y++) {
    for ($x = 1; $x -lt $Size - 1; $x++) {
        if (-not $mask[$x, $y]) { continue }
        $u = ($x / ($Size - 1.0) - 0.5) * 2.0
        $v = ($y / ($Size - 1.0) - 0.5) * 2.0
        $camber = [Math]::Max(0.0, 1.0 - $u * $u) * [Math]::Max(0.0, 1.0 - $v * $v) * 0.16
        $neighbourMean = ($values[($x - 1), $y] + $values[($x + 1), $y] + $values[$x, ($y - 1)] + $values[$x, ($y + 1)]) * 0.25
        $currentValue = $values[$x, $y]
        $vein = [Math]::Max(0.0, $currentValue - $neighbourMean) * 1.65
        $midrib = [Math]::Exp(-[Math]::Pow($u / 0.018, 2.0)) * [Math]::Max(0.0, 1.0 - [Math]::Abs($v)) * 0.08
        $heightValues[$x, $y] = [Math]::Min(1.0, 0.42 + $camber + $vein + $midrib)
    }
}

$normal = [System.Drawing.Bitmap]::new($Size, $Size)
for ($y = 0; $y -lt $Size; $y++) {
    for ($x = 0; $x -lt $Size; $x++) {
        if (-not $mask[$x, $y]) {
            $height.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(255, 128, 128, 128))
            $normal.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(255, 128, 128, 255))
            continue
        }
        $left = $heightValues[[int][Math]::Max(0, $x - 1), $y]
        $right = $heightValues[[int][Math]::Min($Size - 1, $x + 1), $y]
        $up = $heightValues[$x, [int][Math]::Max(0, $y - 1)]
        $down = $heightValues[$x, [int][Math]::Min($Size - 1, $y + 1)]
        $nx = -($right - $left) * 7.0
        $ny = ($down - $up) * 7.0 # DirectX green channel
        $nz = 1.0
        $length = [Math]::Sqrt($nx * $nx + $ny * $ny + $nz * $nz)
        $r = [int](($nx / $length * 0.5 + 0.5) * 255)
        $g = [int](($ny / $length * 0.5 + 0.5) * 255)
        $b = [int](($nz / $length * 0.5 + 0.5) * 255)
        $h = [int]($heightValues[$x, $y] * 255)
        $height.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(255, $h, $h, $h))
        $normal.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(255, $r, $g, $b))
    }
}

$resized.Dispose()
$opacity.Save((Join-Path $OutputDirectory "common_hazel_leaf_opacity.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$albedo.Save((Join-Path $OutputDirectory "common_hazel_leaf_front_albedo.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$albedo.Save((Join-Path $OutputDirectory "common_hazel_leaf_back_albedo.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$height.Save((Join-Path $OutputDirectory "common_hazel_leaf_height.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$normal.Save((Join-Path $OutputDirectory "common_hazel_leaf_front_normal_dx.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$normal.Save((Join-Path $OutputDirectory "common_hazel_leaf_back_normal_dx.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$opacity.Dispose()
$albedo.Dispose()
$height.Dispose()
$normal.Dispose()
