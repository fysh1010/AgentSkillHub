param(
    [Parameter(Mandatory = $true)]
    [string]$ExePath,
    [string]$OutputDirectory = "release"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$projectRoot = Split-Path -Parent $PSScriptRoot
$version = (Get-Content -LiteralPath (Join-Path $projectRoot "package.json") -Raw -Encoding utf8 | ConvertFrom-Json).version
$tauriVersion = (Get-Content -LiteralPath (Join-Path $projectRoot "src-tauri/tauri.conf.json") -Raw -Encoding utf8 | ConvertFrom-Json).version
if ($version -ne $tauriVersion) { throw "package.json and Tauri versions differ" }
if ($env:GITHUB_REF_TYPE -eq "tag" -and $env:GITHUB_REF_NAME -ne "v$version") { throw "Release tag and project version differ" }

$program = Get-Item -LiteralPath $ExePath
if ($program.PSIsContainer -or $program.Extension -ne ".exe") { throw "A built EXE is required" }
$binaryVersion = $program.VersionInfo.FileVersion
if ($binaryVersion -ne $version -and -not $binaryVersion.StartsWith("$version.")) { throw "EXE version differs from project version: $binaryVersion" }

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$zipPath = Join-Path ([System.IO.Path]::GetFullPath($OutputDirectory)) "AgentSkillHub-v$version-portable.zip"
if (Test-Path -LiteralPath $zipPath) { throw "Archive already exists: $zipPath" }

# The same portable package is produced locally and in CI.
$archive = [System.IO.Compression.ZipFile]::Open($zipPath, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $program.FullName, "AgentSkillHub.exe", [System.IO.Compression.CompressionLevel]::Optimal) | Out-Null
} finally { $archive.Dispose() }

$sourceHash = (Get-FileHash -LiteralPath $program.FullName -Algorithm SHA256).Hash
$archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
try {
    if ($archive.Entries.Count -ne 1 -or $archive.Entries[0].FullName -ne "AgentSkillHub.exe") { throw "Unexpected archive contents" }
    $stream = $archive.Entries[0].Open()
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $entryHash = [BitConverter]::ToString($hasher.ComputeHash($stream)).Replace("-", "")
        if ($entryHash -ne $sourceHash) { throw "EXE SHA-256 verification failed" }
    } finally { $stream.Dispose(); $hasher.Dispose() }
} finally { $archive.Dispose() }

[PSCustomObject]@{ Path = $zipPath; Bytes = (Get-Item -LiteralPath $zipPath).Length; ExecutableSHA256 = $sourceHash }
