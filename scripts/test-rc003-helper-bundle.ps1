# Synthetic files only; fixtures stay under ignored target/ for investigation.
# This never launches an executable or changes an installed application.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$verifier = Join-Path $PSScriptRoot 'verify-rc003-helper-bundle.ps1'
$repoRoot = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $repoRoot ('target/helper-bundle-verification/' + [Guid]::NewGuid().ToString('N'))
$utf8 = New-Object Text.UTF8Encoding($false)
$passed = 0

function Write-FixtureManifest($Fixture) {
    [IO.File]::WriteAllText($Fixture.ManifestPath, ($Fixture.Manifest | ConvertTo-Json -Depth 8), $utf8)
}

function New-Fixture([string]$Name) {
    $root = Join-Path $testRoot $Name
    $bundle = Join-Path $root 'bundle'
    New-Item -ItemType Directory -Path $bundle -Force | Out-Null
    $helper = Join-Path $bundle 'SayAllKeyHelper.exe'
    [IO.File]::WriteAllBytes($helper, [byte[]](1, 2, 3))
    $fixture = @{
        Bundle = $bundle; App = (Join-Path $root 'application.exe')
        ManifestPath = (Join-Path $bundle 'manifest.json'); Expected = (Join-Path $root 'expected.json')
        Manifest = @{ schema = 1; name = 'SayAllKeyHelper'; files = @(
            @{ path = 'SayAllKeyHelper.exe'; sha256 = (Get-FileHash $helper -Algorithm SHA256).Hash; size = 3 }
        ) }
    }
    Write-FixtureManifest $fixture
    $manifestBytes = [IO.File]::ReadAllBytes($fixture.ManifestPath)
    [IO.File]::WriteAllBytes($fixture.Expected, $manifestBytes)
    # A minimal section table makes containment testable without compiling or
    # running a PE. The verifier does not claim this synthetic image is runnable.
    $pe = New-Object byte[] (1024 + $manifestBytes.Length)
    $pe[0] = 0x4d; $pe[1] = 0x5a
    [BitConverter]::GetBytes([uint32]128).CopyTo($pe, 0x3c)
    [BitConverter]::GetBytes([uint32]0x4550).CopyTo($pe, 128)
    [BitConverter]::GetBytes([uint16]0x8664).CopyTo($pe, 132)
    [BitConverter]::GetBytes([uint16]1).CopyTo($pe, 134)
    [BitConverter]::GetBytes([uint32]$manifestBytes.Length).CopyTo($pe, 152 + 16)
    [BitConverter]::GetBytes([uint32]1024).CopyTo($pe, 152 + 20)
    [BitConverter]::GetBytes([uint32]0x40000040).CopyTo($pe, 152 + 36)
    $manifestBytes.CopyTo($pe, 1024)
    [IO.File]::WriteAllBytes($fixture.App, $pe)
    return $fixture
}

function Invoke-Case([string]$Name, [scriptblock]$Mutation, [string]$Failure = '') {
    $fixture = New-Fixture $Name
    & $Mutation $fixture
    $message = ''
    try {
        & $verifier -BundleRoot $fixture.Bundle -ApplicationPath $fixture.App -ExpectedManifestPath $fixture.Expected | Out-Null
    } catch { $message = $_.Exception.Message }
    if (($Failure -eq '' -and $message -ne '') -or
        ($Failure -ne '' -and $message -notlike "*reason=$Failure")) { throw "Synthetic case failed: $Name" }
    $script:passed++
}

function Sync-Expected($Fixture) {
    Write-FixtureManifest $Fixture
    [IO.File]::WriteAllBytes($Fixture.Expected, [IO.File]::ReadAllBytes($Fixture.ManifestPath))
}

Invoke-Case 'complete' { param($f) }
Invoke-Case 'empty_manifest' { param($f) [IO.File]::WriteAllBytes($f.ManifestPath, [byte[]]@()) } 'manifest_empty'
Invoke-Case 'empty_files' { param($f) $f.Manifest.files = @(); Sync-Expected $f } 'manifest_schema_invalid'
Invoke-Case 'invalid_schema_type' { param($f) $f.Manifest.schema = '1'; Sync-Expected $f } 'manifest_schema_invalid'
Invoke-Case 'missing_helper' { param($f) Move-Item (Join-Path $f.Bundle 'SayAllKeyHelper.exe') (Join-Path (Split-Path $f.Bundle) 'saved-helper') } 'io_or_schema_error'
Invoke-Case 'tampered_file' { param($f) [IO.File]::WriteAllBytes((Join-Path $f.Bundle 'SayAllKeyHelper.exe'), [byte[]](3, 2, 1)) } 'file_hash_mismatch'
Invoke-Case 'size_mismatch' { param($f) $f.Manifest.files[0].size = 4; Sync-Expected $f } 'file_size_mismatch'
Invoke-Case 'extra_file' { param($f) [IO.File]::WriteAllText((Join-Path $f.Bundle 'extra'), 'x') } 'unlisted_or_duplicate_file'
Invoke-Case 'duplicate_path' { param($f) $f.Manifest.files += @{ path = 'sayallkeyhelper.EXE'; sha256 = $f.Manifest.files[0].sha256 }; Sync-Expected $f } 'duplicate_or_reserved_path'
Invoke-Case 'parent_path' { param($f) $f.Manifest.files[0].path = '../outside'; Sync-Expected $f } 'invalid_path_component'
Invoke-Case 'absolute_path' { param($f) $f.Manifest.files[0].path = $f.App; Sync-Expected $f } 'absolute_path'
Invoke-Case 'alternate_stream' { param($f) $f.Manifest.files[0].path = 'SayAllKeyHelper.exe:stream'; Sync-Expected $f } 'invalid_path_component'
Invoke-Case 'expected_mismatch' { param($f) [IO.File]::WriteAllText($f.Expected, '{}') } 'build_manifest_mismatch'
Invoke-Case 'empty_embedded_manifest' { param($f) $b = [IO.File]::ReadAllBytes($f.App); [Array]::Clear($b, 1024, $b.Length - 1024); [IO.File]::WriteAllBytes($f.App, $b) } 'application_manifest_not_embedded'
Invoke-Case 'overlay_only_manifest' { param($f) $b = [IO.File]::ReadAllBytes($f.App); [BitConverter]::GetBytes([uint32]0).CopyTo($b, 152 + 16); [IO.File]::WriteAllBytes($f.App, $b) } 'application_manifest_not_embedded'
Invoke-Case 'reparse_directory' { param($f) $outside = Join-Path (Split-Path $f.Bundle) 'outside'; New-Item -ItemType Directory -Path $outside | Out-Null; New-Item -ItemType Junction -Path (Join-Path $f.Bundle 'linked') -Target $outside | Out-Null } 'reparse_point'

@{ check = 'rc003_helper_bundle_synthetic'; result = 'passed'; cases = $passed; fixtures_retained = $true } | ConvertTo-Json -Compress
