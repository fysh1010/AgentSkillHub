# 默认只预览；-Apply 仅删除已确认范围内的临时文件，不删除目录或构建环境。
[CmdletBinding(SupportsShouldProcess)]
param([switch]$Apply)

$ErrorActionPreference = "Stop"
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$temporaryRoot = Join-Path $projectRoot "_tmp"
$targets = @()
if (Test-Path -LiteralPath $temporaryRoot) {
    $temporary = Get-Item -LiteralPath $temporaryRoot -Force
    if ($temporary.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "临时目录是链接，已停止" }
    $targets += Get-ChildItem -LiteralPath $temporaryRoot -File -Force | Where-Object {
        $_.Extension -in @(".log", ".png", ".txt", ".json", ".yaml")
    }
    # 已由 scripts/verify-mcp.mjs 等正式验证入口替代的旧调试脚本。
    foreach ($name in @("check-builtin-sources.mjs", "check-market-detail.mjs", "check-ui.mjs", "verify-single-exe.ps1")) {
        $scratchScript = Join-Path $temporaryRoot $name
        if (Test-Path -LiteralPath $scratchScript -PathType Leaf) {
            $targets += Get-Item -LiteralPath $scratchScript -Force
        }
    }
}
$obsoleteScript = Join-Path $projectRoot "cleanup-confirmed-caches.ps1"
if (Test-Path -LiteralPath $obsoleteScript) { $targets += Get-Item -LiteralPath $obsoleteScript -Force }
foreach ($target in $targets) {
    if (-not $target.FullName.StartsWith($projectRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
        ($target.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "目标越界或是链接，已停止" }
    if ($Apply -and $PSCmdlet.ShouldProcess($target.FullName, "删除非必要临时文件")) {
        Remove-Item -LiteralPath $target.FullName -Force
    } else {
        [pscustomobject]@{ Path = $target.FullName; Bytes = $target.Length; Action = "预览" }
    }
}
Write-Host "候选文件 $($targets.Count) 个；不处理任何子目录、依赖、打包产物或运行环境。"
