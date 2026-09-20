param(
    [string]$BundleRoot = (Join-Path (Split-Path -Parent $PSScriptRoot) 'target/release/rc003-helper'),
    [string]$ApplicationPath = (Join-Path (Split-Path -Parent $PSScriptRoot) 'target/release/remote-coding.exe'),
    [string]$ExpectedManifestPath = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$watch = [Diagnostics.Stopwatch]::StartNew()
$stage = 'paths'

function Assert-NoReparseAncestor([string]$Path) {
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor) {
        $item = Get-Item -LiteralPath $cursor -Force -ErrorAction Stop
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'reparse_point' }
        $parent = [IO.Directory]::GetParent($cursor)
        if ($null -eq $parent) { break }
        $cursor = $parent.FullName
    }
}

function Convert-SafeRelativePath([object]$Value) {
    if ($Value -isnot [string] -or [string]::IsNullOrWhiteSpace($Value)) { throw 'invalid_relative_path' }
    $relative = $Value.Replace('\', '/')
    if ([IO.Path]::IsPathRooted($Value) -or $relative.StartsWith('/')) { throw 'absolute_path' }
    foreach ($part in $relative.Split('/')) {
        if ($part -eq '' -or $part -eq '.' -or $part -eq '..' -or
            $part.EndsWith('.') -or $part.EndsWith(' ') -or
            $part.IndexOfAny([IO.Path]::GetInvalidFileNameChars()) -ge 0 -or
            $part -match '^(CON|PRN|AUX|NUL|COM[1-9\u00b9\u00b2\u00b3]|LPT[1-9\u00b9\u00b2\u00b3])(\.|$)') {
            throw 'invalid_path_component'
        }
    }
    return $relative
}

function Assert-EmbeddedManifest([byte[]]$Bytes, [byte[]]$ManifestBytes) {
    # include_str! embeds the exact UTF-8 bytes. Search only file-backed readable
    # initialized PE sections, excluding overlays and arbitrary appended data.
    if ($Bytes.Length -lt 64 -or $Bytes[0] -ne 0x4d -or $Bytes[1] -ne 0x5a) { throw 'application_not_pe' }
    $pe = [BitConverter]::ToUInt32($Bytes, 0x3c)
    if ([long]$pe + 24 -gt $Bytes.Length -or [BitConverter]::ToUInt32($Bytes, $pe) -ne 0x4550) {
        throw 'application_invalid_pe_header'
    }
    if ([BitConverter]::ToUInt16($Bytes, $pe + 4) -ne 0x8664) { throw 'application_not_x64' }
    $count = [BitConverter]::ToUInt16($Bytes, $pe + 6)
    $sectionTable = [long]$pe + 24 + [BitConverter]::ToUInt16($Bytes, $pe + 20)
    if ($count -eq 0 -or $sectionTable + 40 * $count -gt $Bytes.Length) { throw 'application_invalid_sections' }
    # Latin-1 is a one-byte-to-one-character mapping, so this ordinal comparison
    # searches bytes without lossy UTF-8 decoding of the executable.
    $byteEncoding = [Text.Encoding]::GetEncoding(28591)
    $needle = $byteEncoding.GetString($ManifestBytes)
    for ($i = 0; $i -lt $count; $i++) {
        $section = [int]($sectionTable + 40 * $i)
        $length = [BitConverter]::ToUInt32($Bytes, $section + 16)
        $offset = [BitConverter]::ToUInt32($Bytes, $section + 20)
        $flags = [BitConverter]::ToUInt32($Bytes, $section + 36)
        if ([long]$offset + $length -gt $Bytes.Length) { throw 'application_section_out_of_bounds' }
        if (($flags -band 0x40000040) -ne 0x40000040 -or $length -lt $ManifestBytes.Length) { continue }
        if ($byteEncoding.GetString($Bytes, $offset, $length).IndexOf($needle, [StringComparison]::Ordinal) -ge 0) {
            return
        }
    }
    throw 'application_manifest_not_embedded'
}

try {
    $BundleRoot = [IO.Path]::GetFullPath($BundleRoot)
    $ApplicationPath = [IO.Path]::GetFullPath($ApplicationPath)
    Assert-NoReparseAncestor $BundleRoot
    Assert-NoReparseAncestor $ApplicationPath
    if (-not (Test-Path -LiteralPath $BundleRoot -PathType Container) -or
        -not (Test-Path -LiteralPath $ApplicationPath -PathType Leaf)) { throw 'required_path_missing' }
    $rootPrefix = $BundleRoot.TrimEnd([char[]]'\/') + [IO.Path]::DirectorySeparatorChar
    $manifestPath = Join-Path $BundleRoot 'manifest.json'
    Assert-NoReparseAncestor $manifestPath
    $stage = 'manifest'
    $manifestBytes = [IO.File]::ReadAllBytes($manifestPath)
    if ($manifestBytes.Length -eq 0) { throw 'manifest_empty' }
    $utf8 = New-Object Text.UTF8Encoding($false, $true)
    $manifest = $utf8.GetString($manifestBytes) | ConvertFrom-Json
    if ($null -eq $manifest -or $manifest -is [array] -or
        ($manifest.schema -isnot [int] -and $manifest.schema -isnot [long]) -or
        $manifest.schema -ne 1 -or $manifest.name -cne 'SayAllKeyHelper' -or
        $manifest.files -isnot [array] -or $manifest.files.Count -eq 0) { throw 'manifest_schema_invalid' }
    if ($ExpectedManifestPath) {
        Assert-NoReparseAncestor $ExpectedManifestPath
        $expectedBytes = [IO.File]::ReadAllBytes([IO.Path]::GetFullPath($ExpectedManifestPath))
        if ([Convert]::ToBase64String($expectedBytes) -cne [Convert]::ToBase64String($manifestBytes)) {
            throw 'build_manifest_mismatch'
        }
    }

    $stage = 'files'
    $declared = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $sizeFields = 0
    $totalBytes = [long]0
    foreach ($entry in $manifest.files) {
        $relative = Convert-SafeRelativePath $entry.path
        if ($relative -ieq 'manifest.json' -or -not $declared.Add($relative)) { throw 'duplicate_or_reserved_path' }
        if ($entry.sha256 -isnot [string] -or $entry.sha256 -cnotmatch '^[0-9a-fA-F]{64}$') { throw 'invalid_file_hash' }
        $path = [IO.Path]::GetFullPath((Join-Path $BundleRoot $relative))
        if (-not $path.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'path_escaped_bundle' }
        Assert-NoReparseAncestor $path
        $file = Get-Item -LiteralPath $path -Force
        if ($file -isnot [IO.FileInfo]) { throw 'declared_file_missing' }
        $sizeProperty = $entry.PSObject.Properties['size']
        if ($null -ne $sizeProperty) {
            $size = $sizeProperty.Value
            if (($size -isnot [int] -and $size -isnot [long]) -or $size -lt 0 -or $size -ne $file.Length) {
                throw 'file_size_mismatch'
            }
            $sizeFields++
        }
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ine $entry.sha256) { throw 'file_hash_mismatch' }
        $totalBytes += $file.Length
    }
    if (-not $declared.Contains('SayAllKeyHelper.exe') -or
        (Get-Item -LiteralPath (Join-Path $BundleRoot 'SayAllKeyHelper.exe')).Length -eq 0) {
        throw 'helper_executable_missing'
    }
    $observed = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $pending = [Collections.Generic.Stack[string]]::new()
    $pending.Push($BundleRoot)
    while ($pending.Count -gt 0) {
        foreach ($item in Get-ChildItem -LiteralPath $pending.Pop() -Force) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'reparse_point' }
            if ($item -is [IO.DirectoryInfo]) { $pending.Push($item.FullName); continue }
            if ($item -isnot [IO.FileInfo]) { throw 'non_regular_file' }
            $relative = $item.FullName.Substring($rootPrefix.Length).Replace('\', '/')
            if ($relative -ieq 'manifest.json') { continue }
            if (-not $declared.Contains($relative) -or -not $observed.Add($relative)) { throw 'unlisted_or_duplicate_file' }
        }
    }
    if ($observed.Count -ne $declared.Count) { throw 'file_set_mismatch' }
    $stage = 'application_manifest'
    Assert-EmbeddedManifest ([IO.File]::ReadAllBytes($ApplicationPath)) $manifestBytes
    [ordered]@{
        check = 'rc003_helper_bundle'; result = 'passed'; files = $declared.Count
        bytes = $totalBytes; size_fields = $sizeFields; hashes_verified = $true
        expected_manifest_checked = [bool]$ExpectedManifestPath; embedded_manifest_verified = $true
        elapsed_ms = $watch.ElapsedMilliseconds
    } | ConvertTo-Json -Compress
} catch {
    # Exception text can contain user installation paths; only stable stage data
    # enters build logs. The caller receives failure via the terminating error.
    $reason = 'io_or_schema_error'
    if ($_.Exception.Message -cmatch '^[a-z_]+$') { $reason = $_.Exception.Message }
    throw "RC003 helper bundle verification failed: stage=$stage reason=$reason"
}
