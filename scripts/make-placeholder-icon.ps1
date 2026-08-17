# 生成 1024x1024 占位应用图标（浮冰上的匣子，品牌同源几何）
# 后续拿到正式图标后：pnpm tauri icon <正式图标.png> 一条命令替换。
Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.Clear([System.Drawing.Color]::Transparent)

# 圆角矩形辅助
function RoundRect($x, $y, $w, $h, $r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = 2 * $r
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

# 背景：深冰蓝圆角方形
$accentDeep = [System.Drawing.Color]::FromArgb(255, 37, 102, 133)   # #256685
$accentIce  = [System.Drawing.Color]::FromArgb(255, 235, 247, 251)  # 冰白
$bgPath = RoundRect 32 32 960 960 220
$bgBrush = New-Object System.Drawing.SolidBrush($accentDeep)
$g.FillPath($bgBrush, $bgPath)

# 匣体：冰白描边圆角方
$boxPen = New-Object System.Drawing.Pen($accentIce, 66)
$boxPath = RoundRect 168 360 688 460 120
$g.DrawPath($boxPen, $boxPath)

# 匣口：半透明短横
$fade = [System.Drawing.Color]::FromArgb(140, 235, 247, 251)
$lineBrush = New-Object System.Drawing.SolidBrush($fade)
$g.FillRectangle($lineBrush, 360, 545, 304, 52)

# 悬浮冰板：右上小圆角块
$floePath = RoundRect 552 152 316 150 66
$floeBrush = New-Object System.Drawing.SolidBrush($accentIce)
$g.FillPath($floeBrush, $floePath)

$g.Dispose()
$out = "app-icon.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "saved $out"
