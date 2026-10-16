param(
    [Parameter(Mandatory = $true)]
    [string]$ExePath,
    [string]$OutputDirectory = "release"
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
$version = (Get-Content -LiteralPath (Join-Path $projectRoot "package.json") -Raw | ConvertFrom-Json).version
$tauriVersion = (Get-Content -LiteralPath (Join-Path $projectRoot "src-tauri/tauri.conf.json") -Raw | ConvertFrom-Json).version
if ($version -ne $tauriVersion) { throw "package.json 与 Tauri 版本不一致" }
if ($env:GITHUB_REF_TYPE -eq "tag" -and $env:GITHUB_REF_NAME -ne "v$version") { throw "发布标签与项目版本不一致" }

$program = Get-Item -LiteralPath $ExePath
if ($program.PSIsContainer -or $program.Extension -ne ".exe") { throw "请提供构建完成的 EXE" }
$binaryVersion = $program.VersionInfo.FileVersion
if ($binaryVersion -ne $version -and -not $binaryVersion.StartsWith("$version.")) { throw "EXE 版本与项目版本不一致：$binaryVersion" }

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$zipPath = Join-Path ([System.IO.Path]::GetFullPath($OutputDirectory)) "AgentSkillHub-v$version-portable.zip"
if (Test-Path -LiteralPath $zipPath) { throw "目标已存在，未覆盖：$zipPath" }

# 本地和 CI 共用打包入口；ZIP 中只包含可直接运行的程序。
$archive = [System.IO.Compression.ZipFile]::Open($zipPath, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $program.FullName, "AgentSkillHub.exe", [System.IO.Compression.CompressionLevel]::Optimal) | Out-Null
} finally { $archive.Dispose() }

$sourceHash = (Get-FileHash -LiteralPath $program.FullName -Algorithm SHA256).Hash
$archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
try {
    if ($archive.Entries.Count -ne 1 -or $archive.Entries[0].FullName -ne "AgentSkillHub.exe") { throw "压缩包内容不正确" }
    $stream = $archive.Entries[0].Open()
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $entryHash = [BitConverter]::ToString($hasher.ComputeHash($stream)).Replace("-", "")
        if ($entryHash -ne $sourceHash) { throw "压缩包内 EXE 的 SHA-256 校验失败" }
    } finally { $stream.Dispose(); $hasher.Dispose() }
} finally { $archive.Dispose() }

[PSCustomObject]@{ Path = $zipPath; Bytes = (Get-Item -LiteralPath $zipPath).Length; ExecutableSHA256 = $sourceHash }
