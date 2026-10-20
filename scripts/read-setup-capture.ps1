param(
    [string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot),
    [Parameter(Mandatory = $true)][string]$ReceiptPath,
    [Parameter(Mandatory = $true)][string]$Version
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($Version -cnotmatch '^\d+\.\d+\.\d+$') { throw 'Setup capture version is invalid' }
$taskReceiptPath = [IO.Path]::GetFullPath($ReceiptPath)
$taskStage = [IO.Path]::GetDirectoryName($taskReceiptPath)
$taskBuildRoot = [IO.Path]::GetFullPath((Join-Path $RepositoryRoot 'target/setup-build'))
if ([IO.Path]::GetDirectoryName($taskStage) -ine $taskBuildRoot -or
    [IO.Path]::GetFileName($taskReceiptPath) -cne 'capture-receipt.json' -or
    [IO.Path]::GetFileName($taskStage) -cnotmatch ('^' + [regex]::Escape($Version) + '-([0-9a-f]{32})$')) {
    throw 'Setup capture identity does not match its build directory'
}
$taskBuildId = $Matches[1]
foreach ($taskPath in @($taskBuildRoot, $taskStage, $taskReceiptPath)) {
    if (-not (Test-Path -LiteralPath $taskPath)) { throw 'Setup capture receipt is missing' }
    if ((Get-Item -LiteralPath $taskPath).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw 'Setup capture must not use links'
    }
}
$taskText = [IO.File]::ReadAllText($taskReceiptPath)
if (-not $taskText.TrimStart().StartsWith('{', [StringComparison]::Ordinal)) { throw 'Setup capture receipt must be an object' }
$taskCapture = $taskText | ConvertFrom-Json
if ($null -eq $taskCapture -or $taskCapture -isnot [pscustomobject]) { throw 'Setup capture receipt must be an object' }
foreach ($taskField in @('schema_version', 'version', 'build_id', 'installed_app_sha256', 'installed_app_size_bytes')) {
    if ($taskField -notin @($taskCapture.PSObject.Properties.Name)) { throw "Setup capture is missing $taskField" }
}
if (($taskCapture.schema_version -isnot [int] -and $taskCapture.schema_version -isnot [long]) -or $taskCapture.schema_version -ne 1) {
    throw 'Setup capture schema is unsupported'
}
if ($taskCapture.version -isnot [string] -or $taskCapture.version -cne $Version -or
    $taskCapture.build_id -isnot [string] -or $taskCapture.build_id -cne $taskBuildId) {
    throw 'Setup capture version or build identity does not match'
}
if ($taskCapture.installed_app_sha256 -isnot [string] -or $taskCapture.installed_app_sha256 -cnotmatch '^[0-9a-f]{64}$') {
    throw 'Setup capture application hash is invalid'
}
if (($taskCapture.installed_app_size_bytes -isnot [int] -and $taskCapture.installed_app_size_bytes -isnot [long]) -or
    $taskCapture.installed_app_size_bytes -le 0) {
    throw 'Setup capture application size is invalid'
}
$taskSnapshot = Join-Path $taskStage 'installed-app.exe'
if (-not (Test-Path -LiteralPath $taskSnapshot -PathType Leaf)) { throw 'Setup captured application is missing' }
$taskFile = Get-Item -LiteralPath $taskSnapshot
if ($taskFile.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Setup capture must not use links' }
if ($taskFile.Length -ne $taskCapture.installed_app_size_bytes -or
    (Get-FileHash -LiteralPath $taskSnapshot -Algorithm SHA256).Hash.ToLowerInvariant() -cne $taskCapture.installed_app_sha256) {
    throw 'Setup captured application differs from its receipt'
}
[pscustomobject][ordered]@{
    version = $Version
    build_id = $taskBuildId
    installed_app_sha256 = $taskCapture.installed_app_sha256
    capture_receipt_sha256 = (Get-FileHash -LiteralPath $taskReceiptPath -Algorithm SHA256).Hash.ToLowerInvariant()
}
