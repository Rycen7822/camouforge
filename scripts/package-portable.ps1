# 便携发布打包：生成 dist\camouforge\（exe + worker + Python 自举输入 + LICENSE）。
# 发布包不预装 .venv / .camouforge-runtime / Python / uv 缓存 / Camoufox 浏览器——
# 这些由应用首次启动时自动创建。
#
# 用法：
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\package-portable.ps1
#   powershell ... -File scripts\package-portable.ps1 -NoBuild   # 复用已有 release exe
param(
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"

$repo = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$distRoot = [System.IO.Path]::GetFullPath((Join-Path $repo "dist"))
$target = [System.IO.Path]::GetFullPath((Join-Path $distRoot "camouforge"))

# 删除/覆盖前校验目标确实位于仓库 dist 下，防止误清其他路径
if (-not ($distRoot.StartsWith((Join-Path $repo ""), [System.StringComparison]::OrdinalIgnoreCase))) {
    throw "dist 路径异常：$distRoot"
}
if (-not ($target.StartsWith($distRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase) -or $target -eq $distRoot)) {
    throw "打包目标异常：$target 不在 $distRoot 下"
}

if (-not $NoBuild) {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release 失败" }
}

$sources = @(
    @{ Src = Join-Path $repo "target\release\camoforge.exe";  Dst = "camoforge.exe" },
    @{ Src = Join-Path $repo "pyproject.toml";                Dst = "pyproject.toml" },
    @{ Src = Join-Path $repo "uv.lock";                       Dst = "uv.lock" },
    @{ Src = Join-Path $repo "uv.toml";                       Dst = "uv.toml" },
    @{ Src = Join-Path $repo "LICENSE";                       Dst = "LICENSE" }
)
$sources += Get-ChildItem -LiteralPath (Join-Path $repo "worker") -Filter "*.py" -File |
    ForEach-Object { @{ Src = $_.FullName; Dst = "worker\$($_.Name)" } }

foreach ($s in $sources) {
    if (-not (Test-Path $s.Src)) {
        throw "缺少必需文件：$($s.Src)"
    }
}

if (Test-Path $target) {
    Remove-Item -Recurse -Force $target
}
New-Item -ItemType Directory -Force -Path (Join-Path $target "worker") | Out-Null

foreach ($s in $sources) {
    $dstPath = Join-Path $target $s.Dst
    Copy-Item -Force $s.Src $dstPath
}

# 自检：发布包不得包含任何预装运行时产物
$forbidden = @(".venv", ".camouforge-runtime", "camoufox")
foreach ($name in $forbidden) {
    if (Test-Path (Join-Path $target $name)) {
        throw "发布包不得包含 $name"
    }
}

Write-Host "打包完成：$target"
Get-ChildItem -Recurse $target | ForEach-Object { Write-Host ("  " + $_.FullName.Substring($target.Length + 1)) }
