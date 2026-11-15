[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidatePattern('^r\d+$')][string]$Before,
    [Parameter(Mandatory = $true)][ValidatePattern('^r\d+$')][string]$After,
    [string[]]$Names = @()
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot)
$beforeDir = Join-Path $root "cook\stills\$Before"
$afterDir = Join-Path $root "cook\stills\$After"
$outDir = Join-Path $root "cook\compare\$After"
foreach ($dir in @($beforeDir, $afterDir)) {
    if (-not (Test-Path -LiteralPath $dir -PathType Container)) { throw "Round directory not found: $dir" }
}

$beforeFiles = Get-ChildItem -LiteralPath $beforeDir -File -Filter '*.png'
$afterFiles = Get-ChildItem -LiteralPath $afterDir -File -Filter '*.png'
$afterMap = @{}
foreach ($file in $afterFiles) { $afterMap[$file.Name.ToLowerInvariant()] = $file }
$requested = @{}
foreach ($item in $Names) {
    foreach ($name in ($item -split ',')) {
        $value = $name.Trim()
        if ($value) {
            if (-not $value.EndsWith('.png', [StringComparison]::OrdinalIgnoreCase)) { $value += '.png' }
            $requested[$value.ToLowerInvariant()] = $true
        }
    }
}
$pairs = @($beforeFiles | Where-Object {
    $key = $_.Name.ToLowerInvariant()
    $afterMap.ContainsKey($key) -and ($requested.Count -eq 0 -or $requested.ContainsKey($key))
})
if ($requested.Count -gt 0) {
    $found = @($pairs | ForEach-Object { $_.Name.ToLowerInvariant() })
    $missing = @($requested.Keys | Where-Object { $_ -notin $found })
    if ($missing.Count) { throw "No matching image in both rounds: $($missing -join ', ')" }
}
if ($pairs.Count -eq 0) { throw "No matching PNGs between $Before and $After" }

Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
foreach ($leftFile in $pairs) {
        $rightFile = $afterMap[$leftFile.Name.ToLowerInvariant()]
        $left = [Drawing.Bitmap]::FromFile($leftFile.FullName)
        $right = [Drawing.Bitmap]::FromFile($rightFile.FullName)
        try {
            $labelHeight = 24
            $gap = 8
            $width = $left.Width + $gap + $right.Width
            $height = $labelHeight + [Math]::Max($left.Height, $right.Height)
            $image = New-Object Drawing.Bitmap($width, $height)
            $graphics = [Drawing.Graphics]::FromImage($image)
            try {
                $graphics.Clear([Drawing.Color]::FromArgb(48, 48, 48))
                $graphics.DrawString("$Before $($leftFile.Name)", [Drawing.SystemFonts]::DefaultFont, [Drawing.Brushes]::White, 4, 4)
                $graphics.DrawString("$After $($leftFile.Name)", [Drawing.SystemFonts]::DefaultFont, [Drawing.Brushes]::White, ($left.Width + $gap + 4), 4)
                $graphics.DrawImage($left, 0, $labelHeight)
                $graphics.DrawImage($right, ($left.Width + $gap), $labelHeight)
                $image.Save((Join-Path $outDir $leftFile.Name), [Drawing.Imaging.ImageFormat]::Png)
            } finally { $graphics.Dispose(); $image.Dispose() }
        } finally { $left.Dispose(); $right.Dispose() }
        Write-Host "Compared $($leftFile.Name)"
}
Write-Host "Built $($pairs.Count) side-by-side PNGs in $outDir"
