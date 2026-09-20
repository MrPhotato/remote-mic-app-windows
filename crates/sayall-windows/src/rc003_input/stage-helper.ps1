# Executed from an embedded, encoded command. No user-writable script is elevated.
$ErrorActionPreference = 'Stop'
$stageExitCode = 21 # Manifest read.
try {
$sourceRoot = '__SOURCE__'
$manifestBytes = [IO.File]::ReadAllBytes((Join-Path $sourceRoot 'manifest.json'))
$stageExitCode = 31 # Manifest hash verification.
$hasher = [Security.Cryptography.SHA256]::Create()
try { $actualDigest = ([BitConverter]::ToString($hasher.ComputeHash($manifestBytes))).Replace('-', '').ToLowerInvariant() }
finally { $hasher.Dispose() }
if ($actualDigest -ne '__DIGEST__') { throw 'manifest_integrity_failed' }
$stageExitCode = 32 # Manifest decoding.
$manifestText = [Text.Encoding]::UTF8.GetString($manifestBytes).TrimStart([char]0xfeff)
$manifest = $manifestText | ConvertFrom-Json
$stageExitCode = 33 # Protected location and security principals.
$stageRoot = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'SayAll RC003 Input Helper'
$stage = Join-Path $stageRoot '__DIGEST__'
$admin = New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')
$system = New-Object Security.Principal.SecurityIdentifier('S-1-5-18')
function Set-ProtectedDirectory([string]$path) {
    if (Test-Path -LiteralPath $path) {
        if ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'unsafe_stage' }
    } else { New-Item -ItemType Directory -Path $path | Out-Null }
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true, $false)
    $acl.SetOwner($admin)
    foreach ($sid in @($admin, $system)) {
        $rule = New-Object Security.AccessControl.FileSystemAccessRule($sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $acl.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $path -AclObject $acl
}
function Set-ProtectedFile([string]$path) {
    $acl = New-Object Security.AccessControl.FileSecurity
    $acl.SetAccessRuleProtection($true, $false)
    $acl.SetOwner($admin)
    foreach ($sid in @($admin, $system)) {
        $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($sid, 'FullControl', 'Allow')))
    }
    Set-Acl -LiteralPath $path -AclObject $acl
}
$stageExitCode = 22 # Protected directory and ACL setup.
# Every parent below Program Files is checked before traversing it.
Set-ProtectedDirectory $stageRoot
Set-ProtectedDirectory $stage
$stageExitCode = 23 # Copy, path checks, and per-file integrity.
foreach ($entry in $manifest.files) {
    $relative = [string]$entry.path
    if ([IO.Path]::IsPathRooted($relative) -or $relative -match '(^|[\\/])\.\.([\\/]|$)' -or $relative.Contains(':')) { throw 'invalid_manifest' }
    $destination = [IO.Path]::GetFullPath((Join-Path $stage $relative))
    if (-not $destination.StartsWith($stage + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'invalid_manifest' }
    $parent = Split-Path -Parent $destination
    $segments = $parent.Substring($stage.Length).TrimStart('\').Split('\', [StringSplitOptions]::RemoveEmptyEntries)
    $cursor = $stage
    foreach ($segment in $segments) { $cursor = Join-Path $cursor $segment; Set-ProtectedDirectory $cursor }
    if (Test-Path -LiteralPath $destination) {
        if ((Get-Item -LiteralPath $destination -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'unsafe_stage' }
        Set-ProtectedFile $destination
    }
    $valid = (Test-Path -LiteralPath $destination) -and ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -eq $entry.sha256)
    if (-not $valid) {
        Copy-Item -LiteralPath (Join-Path $sourceRoot $relative) -Destination $destination -Force
        Set-ProtectedFile $destination
        if ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ne $entry.sha256) { throw 'payload_integrity_failed' }
    }
}
# Verify the complete copied payload before executing any of it; never elevate from sourceRoot.
$stageExitCode = 23 # Copy, path checks, and per-file integrity.
foreach ($entry in $manifest.files) {
    if ((Get-FileHash -LiteralPath (Join-Path $stage $entry.path) -Algorithm SHA256).Hash -ne $entry.sha256) { throw 'payload_integrity_failed' }
}
$stageExitCode = 24 # Start fixed helper executable.
$helper = Start-Process -FilePath (Join-Path $stage 'SayAllKeyHelper.exe') -WorkingDirectory $stage -ArgumentList @('--port','__PORT__','--token','__TOKEN__','--parent-pid','__PARENT__') -WindowStyle Hidden -PassThru
$helper.WaitForExit()
exit $helper.ExitCode

} catch { exit $stageExitCode }
