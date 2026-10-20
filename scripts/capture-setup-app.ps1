param(
    [Parameter(Mandatory = $true)][string]$SourcePath,
    [Parameter(Mandatory = $true)][string]$ReceiptPath,
    [Parameter(Mandatory = $true)][string]$Version
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$taskRepo = Split-Path -Parent $PSScriptRoot
if ($Version -cnotmatch '^\d+\.\d+\.\d+$') { throw 'Setup capture version is invalid' }
$taskReceipt = [IO.Path]::GetFullPath($ReceiptPath)
$taskStage = [IO.Path]::GetDirectoryName($taskReceipt)
$taskBuildRoot = [IO.Path]::GetFullPath((Join-Path $taskRepo 'target/setup-build'))
$taskStageName = [IO.Path]::GetFileName($taskStage)
if ([IO.Path]::GetDirectoryName($taskStage) -ine $taskBuildRoot -or
    [IO.Path]::GetFileName($taskReceipt) -cne 'capture-receipt.json' -or
    $taskStageName -cnotmatch ('^' + [regex]::Escape($Version) + '-([0-9a-f]{32})$')) {
    throw 'Setup capture must use the fresh version-bound build directory'
}
$taskBuildId = $Matches[1]
if (-not (Test-Path -LiteralPath $taskStage -PathType Container) -or
    (Get-Item -LiteralPath $taskStage).Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw 'Setup capture directory is missing or is a link'
}
$taskSource = (Resolve-Path -LiteralPath $SourcePath).Path
$taskTargetRoot = [IO.Path]::GetFullPath((Join-Path $taskRepo 'target')).TrimEnd('\') + '\'
if (-not $taskSource.StartsWith($taskTargetRoot, [StringComparison]::OrdinalIgnoreCase) -or
    (Get-Item -LiteralPath $taskSource).Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw 'Setup capture source must be a regular build output'
}
$taskSnapshot = Join-Path $taskStage 'installed-app.exe'
if ((Test-Path -LiteralPath $taskReceipt) -or (Test-Path -LiteralPath $taskSnapshot)) {
    throw 'Setup capture already exists; use a new build directory'
}
# Tauri patches the bundle marker and may sign the executable before NSIS reads
# it, then restores the build output. Capture exactly those packaged bytes here.
# Holding a read-only share prevents a writer from changing bytes during capture.
$taskSourceStream = [IO.File]::Open($taskSource, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
try {
    $taskSnapshotStream = [IO.File]::Open($taskSnapshot, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $taskSourceStream.CopyTo($taskSnapshotStream); $taskSnapshotStream.Flush() }
    finally { $taskSnapshotStream.Dispose() }
    # Use .NET directly: makensis may launch 32-bit Windows PowerShell with a
    # PSModulePath inherited from PowerShell 7, where Get-FileHash is unavailable.
    $taskSha = [Security.Cryptography.SHA256]::Create()
    try {
        $taskSourceStream.Position = 0
        $taskHash = [BitConverter]::ToString($taskSha.ComputeHash($taskSourceStream)).Replace('-', '').ToLowerInvariant()
        $taskSnapshotRead = [IO.File]::OpenRead($taskSnapshot)
        try { $taskSnapshotHash = [BitConverter]::ToString($taskSha.ComputeHash($taskSnapshotRead)).Replace('-', '').ToLowerInvariant() }
        finally { $taskSnapshotRead.Dispose() }
    } finally { $taskSha.Dispose() }
    if ($taskHash -cne $taskSnapshotHash) {
        throw 'Setup capture changed while copying'
    }
    $taskCapture = [ordered]@{
        schema_version = 1
        version = $Version
        build_id = $taskBuildId
        installed_app_sha256 = $taskHash
        installed_app_size_bytes = $taskSourceStream.Length
    } | ConvertTo-Json
    $taskReceiptStream = [IO.File]::Open($taskReceipt, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $taskBytes = [Text.UTF8Encoding]::new($false).GetBytes($taskCapture)
        $taskReceiptStream.Write($taskBytes, 0, $taskBytes.Length)
    } finally { $taskReceiptStream.Dispose() }
} finally { $taskSourceStream.Dispose() }
Write-Host 'Setup packaged application capture: passed.'
