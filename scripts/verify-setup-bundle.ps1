param(
    [string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot),
    [Parameter(Mandatory = $true)][string]$InstallerPath,
    [switch]$RequireUpdaterSignature
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$taskConfigPath = Join-Path $RepositoryRoot 'src-tauri/tauri.conf.json'
$taskVersion = [string](Get-Content -LiteralPath $taskConfigPath -Raw | ConvertFrom-Json).version
if ($taskVersion -cnotmatch '^\d+\.\d+\.\d+$') { throw 'Setup bundle configuration version is invalid' }
$taskBundleDirectory = [IO.Path]::GetFullPath((Join-Path $RepositoryRoot 'target/release/bundle/nsis'))
$taskInstaller = [IO.Path]::GetFullPath($InstallerPath)
if ([IO.Path]::GetDirectoryName($taskInstaller) -ine $taskBundleDirectory -or
    [IO.Path]::GetFileName($taskInstaller) -cne "Remote Coding_$($taskVersion)_x64-setup.exe") {
    throw 'Setup bundle installer identity does not match the current version'
}
$taskReceiptPath = Join-Path $RepositoryRoot 'target/release/setup-build-receipt.json'
if (-not (Test-Path -LiteralPath $taskReceiptPath -PathType Leaf)) {
    throw 'Final setup bundle receipt is missing; do not publish the inner NSIS installer'
}
$taskReceiptText = [IO.File]::ReadAllText($taskReceiptPath)
# PowerShell's JSON pipeline unwraps a one-element array. Reject its top-level
# shape before conversion, so an array cannot masquerade as a schema object.
if (-not $taskReceiptText.TrimStart().StartsWith('{', [StringComparison]::Ordinal)) {
    throw 'Setup bundle receipt must be an object'
}
$taskReceipt = $taskReceiptText | ConvertFrom-Json
if ($null -eq $taskReceipt -or $taskReceipt -is [array] -or $taskReceipt -isnot [pscustomobject]) {
    throw 'Setup bundle receipt must be an object'
}
$taskProperties = @($taskReceipt.PSObject.Properties.Name)
foreach ($taskField in @('schema_version', 'version', 'installer_sha256', 'payload_sha256', 'app_sha256', 'host_sha256', 'webview_bootstrapper_sha256', 'updater_signed')) {
    if ($taskField -notin $taskProperties) { throw "Setup bundle receipt is missing $taskField" }
}
if (($taskReceipt.schema_version -isnot [int] -and $taskReceipt.schema_version -isnot [long]) -or $taskReceipt.schema_version -ne 1) {
    throw 'Setup bundle receipt schema is unsupported'
}
if ($taskReceipt.version -isnot [string] -or $taskReceipt.version -cne $taskVersion) {
    throw 'Setup bundle receipt version does not match the current version'
}
foreach ($taskField in @('installer_sha256', 'payload_sha256', 'app_sha256', 'host_sha256', 'webview_bootstrapper_sha256')) {
    if ($taskReceipt.$taskField -isnot [string] -or $taskReceipt.$taskField -cnotmatch '^[0-9a-f]{64}$') {
        throw "Setup bundle receipt hash is invalid: $taskField"
    }
}
if ($taskReceipt.updater_signed -isnot [bool]) { throw 'Setup bundle updater_signed must be a boolean' }
if ($taskReceipt.installer_sha256 -ceq $taskReceipt.payload_sha256) {
    throw 'Setup bundle receipt identifies the inner payload as the final installer'
}
$taskFiles = [ordered]@{
    installer_sha256 = $taskInstaller
    app_sha256 = Join-Path $RepositoryRoot 'target/release/remote-coding.exe'
    host_sha256 = Join-Path $RepositoryRoot 'target/release/sayall-setup.exe'
}
foreach ($taskEntry in $taskFiles.GetEnumerator()) {
    if (-not (Test-Path -LiteralPath $taskEntry.Value -PathType Leaf)) {
        throw "Setup bundle file is missing: $($taskEntry.Key)"
    }
    $taskActual = (Get-FileHash -LiteralPath $taskEntry.Value -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($taskActual -cne $taskReceipt.($taskEntry.Key)) {
        throw "Setup bundle file hash differs from the receipt: $($taskEntry.Key)"
    }
}
if ($RequireUpdaterSignature) {
    if (-not $taskReceipt.updater_signed) { throw 'Final setup bundle was not signed for publication' }
    $taskSignaturePath = $taskInstaller + '.sig'
    if (-not (Test-Path -LiteralPath $taskSignaturePath -PathType Leaf) -or
        [string]::IsNullOrWhiteSpace([IO.File]::ReadAllText($taskSignaturePath))) {
        throw 'Final setup bundle signature is missing or empty'
    }
}
# This receipt proves which bytes were packaged, not cryptographic authenticity.
# The existing independent release verifier must still verify the final signature.
# Return only approved metadata: never copy extra local receipt fields into assets.
[pscustomobject][ordered]@{
    schema_version = 1
    version = $taskVersion
    installer_sha256 = $taskReceipt.installer_sha256
    payload_sha256 = $taskReceipt.payload_sha256
    app_sha256 = $taskReceipt.app_sha256
    host_sha256 = $taskReceipt.host_sha256
    webview_bootstrapper_sha256 = $taskReceipt.webview_bootstrapper_sha256
    updater_signed = $taskReceipt.updater_signed
}
