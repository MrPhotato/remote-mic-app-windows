#Requires -Version 5.1
<#
.SYNOPSIS
Prepare a local test-signed RC003 filter package without trusting or installing it.
.DESCRIPTION
Creates one non-exportable CurrentUser/My code-signing key. Only its public
certificate leaves that store. No trust import, driver installation, BCD change,
restart, timestamp service or network request is performed by this script.
#>
[CmdletBinding()]
param(
    [string]$InputDirectory,
    [string]$ReceiptPath,
    [string]$SignToolPath,
    [string]$Inf2CatPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$targetRoot = Join-Path $repoRoot 'target/sayall-hid-filter'
$outputDirectory = Join-Path $targetRoot 'local-test-signed'
$sourceDirectory = Join-Path $repoRoot 'drivers/sayall-hid-filter'
$runId = [Guid]::NewGuid().ToString('D')
$logDirectory = Join-Path $targetRoot ('test-signing-logs/' + $runId)
$hardwareId = 'HID\{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&012717_PID&32b8_REV&00a4'
$extensionId = '{25b528e7-f8e7-4f92-9d0d-f56458dda41c}'
$payloadNames = @('SayAllHidFilter.inf', 'SayAllHidFilter.sys', 'SayAllHidFilter.cat')
$sourceNames = @('driver.c', 'driver.h', 'LICENSE', 'remap.c', 'remap.h', 'SayAllHidFilter.inf', 'SayAllHidFilter.vcxproj')
$stages = New-Object 'System.Collections.Generic.List[object]'
$certificate = $null
$currentStage = 'preflight'

if (-not $InputDirectory) { $InputDirectory = Join-Path $targetRoot 'x64/Release/SayAllHidFilter' }
$InputDirectory = [IO.Path]::GetFullPath($InputDirectory)
if (-not $ReceiptPath) { $ReceiptPath = Join-Path (Split-Path -Parent $InputDirectory) 'build-receipt.json' }
$ReceiptPath = [IO.Path]::GetFullPath($ReceiptPath)
if (-not $SignToolPath) {
    $SignToolPath = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin/10.0.26100.0/x64/signtool.exe'
}
if (-not $Inf2CatPath) {
    $Inf2CatPath = Join-Path $repoRoot 'target/local-launch/driver-toolchain/packages/Microsoft.Windows.WDK.x64.10.0.26100.6584/c/bin/10.0.26100.0/x86/Inf2Cat.exe'
}

function Assert-NoReparsePath([string]$Path) {
    $probe = [IO.Path]::GetFullPath($Path)
    while ($probe) {
        if (Test-Path -LiteralPath $probe) {
            $item = Get-Item -LiteralPath $probe -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw 'Reparse points are not allowed in package, source, output or tool paths.'
            }
        }
        $parent = [IO.Path]::GetDirectoryName($probe)
        if ($parent -eq $probe) { break }
        $probe = $parent
    }
}

function Assert-FileHash([string]$Path, [string]$Expected) {
    Assert-NoReparsePath $Path
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw 'An expected file is missing.' }
    if ($Expected -notmatch '^[0-9a-fA-F]{64}$') { throw 'Receipt contains an invalid SHA-256.' }
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ine $Expected) {
        throw ('SHA-256 mismatch for fixed file: ' + [IO.Path]::GetFileName($Path))
    }
}

function Write-Stage([string]$Name, [int]$ExitCode, [long]$ElapsedMs) {
    $entry = [ordered]@{ stage = $Name; exit_code = $ExitCode; elapsed_ms = $ElapsedMs }
    $stages.Add($entry)
    $entry | ConvertTo-Json -Compress | Add-Content -LiteralPath (Join-Path $logDirectory 'stages.jsonl') -Encoding UTF8
    $result = if ($ExitCode -eq 0) { 'passed' } else { 'failed' }
    Write-Host "stage=$Name result=$result exit_code=$ExitCode elapsed_ms=$ElapsedMs"
}

function Invoke-NativeStage([string]$Name, [string]$Executable, [string[]]$Arguments) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    & $Executable @Arguments *> (Join-Path $logDirectory ($Name + '.log'))
    $code = $LASTEXITCODE
    Write-Stage $Name $code $watch.ElapsedMilliseconds
    if ($code -ne 0) { throw "Native stage failed: $Name (exit code $code)." }
}

function Read-SignerMetadata([string]$Path, [string]$ExpectedThumbprint) {
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($null -eq $signature.SignerCertificate -or $signature.SignerCertificate.Thumbprint -ine $ExpectedThumbprint) {
        throw 'Signed file does not identify the newly created test certificate.'
    }
    if ([string]$signature.Status -in @('NotSigned', 'HashMismatch')) { throw 'The embedded signature is missing or the file digest mismatches.' }
    # This is signer metadata, not a claim that the currently untrusted chain passes.
    return [ordered]@{ name = [IO.Path]::GetFileName($Path); status = [string]$signature.Status; signer_thumbprint = $ExpectedThumbprint }
}

try {
    foreach ($path in @($InputDirectory, $ReceiptPath, $sourceDirectory, $outputDirectory, $logDirectory, $SignToolPath, $Inf2CatPath)) {
        Assert-NoReparsePath $path
    }
    if (Test-Path -LiteralPath $outputDirectory) { throw 'The fixed output directory already exists; it will not be overwritten.' }
    if (-not (Test-Path -LiteralPath $InputDirectory -PathType Container)) { throw 'The input package directory is missing.' }
    foreach ($tool in @($SignToolPath, $Inf2CatPath)) {
        if (-not (Test-Path -LiteralPath $tool -PathType Leaf)) { throw 'A required local SDK/WDK tool is missing.' }
    }
    $receipt = Get-Content -LiteralPath $ReceiptPath -Raw | ConvertFrom-Json
    if ($receipt.schema -ne 1 -or $receipt.signing -ne 'unsigned' -or $receipt.configuration -ne 'Release|x64') { throw 'Expected an unsigned schema-1 Release|x64 build receipt.' }
    if ($receipt.source_revision -notmatch '^[0-9a-fA-F]{40}$' -or $receipt.reviewed_commit -notmatch '^[0-9a-fA-F]{40}$') { throw 'Receipt commit identities are invalid.' }
    if ($receipt.source_hashes_match_reviewed_commit -ne $true) { throw 'The receipt does not identify reviewed source hashes.' }
    $head = (& git -C $repoRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $head -notmatch '^[0-9a-fA-F]{40}$') { throw 'Unable to resolve repository HEAD.' }
    & git -C $repoRoot merge-base --is-ancestor $receipt.reviewed_commit HEAD
    if ($LASTEXITCODE -ne 0) { throw 'HEAD must contain the reviewed driver commit.' }

    $packageItems = @(Get-ChildItem -LiteralPath $InputDirectory -Force)
    $allowedInputNames = $payloadNames + @('LICENSE', 'README.md', 'build-receipt.json')
    foreach ($item in $packageItems) {
        Assert-NoReparsePath $item.FullName
        if ($item.PSIsContainer -or $allowedInputNames -inotcontains $item.Name) { throw 'Unexpected input-package entry; only the fixed driver payload and known accompanying documents are accepted.' }
    }
    $artifactEntries = @($receipt.artifacts)
    if ($artifactEntries.Count -ne 3) { throw 'Receipt must describe exactly the fixed INF, SYS and CAT.' }
    foreach ($name in $payloadNames) {
        $entry = @($artifactEntries | Where-Object { $_.name -ieq $name })
        if ($entry.Count -ne 1) { throw 'Missing, duplicate or unexpected driver artifact in receipt.' }
        $file = Join-Path $InputDirectory $name
        Assert-FileHash $file $entry[0].sha256
        if ((Get-Item -LiteralPath $file).Length -ne $entry[0].bytes) { throw 'Driver artifact size does not match its receipt.' }
    }
    $sourceEntries = @($receipt.source_files)
    $actualSourceNames = @(Get-ChildItem -LiteralPath $sourceDirectory -File | ForEach-Object { $_.Name })
    if ($sourceEntries.Count -ne $sourceNames.Count -or @(Compare-Object $sourceNames $actualSourceNames).Count -ne 0) { throw 'Driver source set differs from the reviewed fixed set.' }
    foreach ($name in $sourceNames) {
        $entry = @($sourceEntries | Where-Object { $_.name -ieq $name })
        if ($entry.Count -ne 1) { throw 'Missing or duplicate driver source in receipt.' }
        $file = Join-Path $sourceDirectory $name
        Assert-FileHash $file $entry[0].sha256
        $relative = 'drivers/sayall-hid-filter/' + $name
        $reviewedBlob = (& git -C $repoRoot rev-parse ($receipt.reviewed_commit + ':' + $relative)).Trim()
        if ($LASTEXITCODE -ne 0) { throw 'Reviewed source blob is unavailable.' }
        $workingBlob = (& git -C $repoRoot hash-object ('--path=' + $relative) $file).Trim()
        if ($LASTEXITCODE -ne 0 -or $workingBlob -ne $reviewedBlob) { throw 'Current driver source does not match the reviewed commit.' }
    }
    $licenseEntry = @($sourceEntries | Where-Object { $_.name -eq 'LICENSE' })[0]
    Assert-FileHash (Join-Path $InputDirectory 'LICENSE') $licenseEntry.sha256
    $inf = Get-Content -LiteralPath (Join-Path $InputDirectory 'SayAllHidFilter.inf') -Raw
    $activeInf = (($inf -split '\r?\n' | ForEach-Object { ($_ -split ';', 2)[0].Trim() }) -join "`n")
    $modelLines = @($activeInf -split "`n" | Where-Object { $_ -match '^%DeviceDesc%\s*=' })
    if ($modelLines.Count -ne 1 -or $modelLines[0] -ine ('%DeviceDesc%=SayAllFilter_Install, ' + $hardwareId)) { throw 'INF must match only the reviewed REV00a4 hardware ID.' }
    if ($activeInf -notmatch ('(?im)^ExtensionId=' + [regex]::Escape($extensionId) + '$') -or
        $activeInf -notmatch '(?im)^AddService=SayAllHidFilter,,SayAllFilter_Service$' -or
        $activeInf -notmatch '(?im)^CatalogFile=SayAllHidFilter\.cat$') { throw 'INF service, extension or catalog identity differs from the fixed target.' }

    New-Item -ItemType Directory -Path $logDirectory | Out-Null
    Write-Stage 'preflight' 0 0
    $currentStage = 'inf2cat-help'
    # Inf2Cat -h is unsupported; its documented /? help must advertise this OS.
    Invoke-NativeStage $currentStage $Inf2CatPath @('/?')
    if ((Get-Content -LiteralPath (Join-Path $logDirectory 'inf2cat-help.log') -Raw) -notmatch '\b10_GE_X64\b') { throw 'Installed Inf2Cat does not advertise Windows 11 24H2 x64 support.' }

    $currentStage = 'copy-fixed-payload'
    New-Item -ItemType Directory -Path $outputDirectory | Out-Null
    # Verify the old CAT above, but regenerate it from the newly signed SYS below.
    foreach ($name in @('SayAllHidFilter.inf', 'SayAllHidFilter.sys', 'LICENSE')) {
        Copy-Item -LiteralPath (Join-Path $InputDirectory $name) -Destination (Join-Path $outputDirectory $name)
    }
    Write-Stage $currentStage 0 0

    $currentStage = 'create-nonexportable-certificate'
    $certificate = New-SelfSignedCertificate -Type CodeSigningCert -Subject ('CN=SayAll RC003 Local Driver Test-' + $runId) -CertStoreLocation 'Cert:\CurrentUser\My' -Provider 'Microsoft Software Key Storage Provider' -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -KeyUsage DigitalSignature -NotAfter (Get-Date).AddYears(1)
    if (-not $certificate.HasPrivateKey -or $certificate.Thumbprint -notmatch '^[0-9a-fA-F]{40}$') { throw 'The new signing certificate is incomplete.' }
    $certificateFile = 'SayAllRc003LocalTest.cer'
    Export-Certificate -Cert $certificate -FilePath (Join-Path $outputDirectory $certificateFile) -Type CERT | Out-Null
    $publicCertificate = New-Object Security.Cryptography.X509Certificates.X509Certificate2 (Join-Path $outputDirectory $certificateFile)
    try {
        if ($publicCertificate.HasPrivateKey -or $publicCertificate.Thumbprint -ne $certificate.Thumbprint) { throw 'Public certificate export validation failed.' }
    } finally { $publicCertificate.Dispose() }
    Write-Stage $currentStage 0 0
    $thumbprint = $certificate.Thumbprint
    [ordered]@{ schema = 1; run_id = $runId; certificate_thumbprint = $thumbprint; private_key_exported = $false; trust_imported = $false } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $logDirectory 'certificate-receipt.json') -Encoding UTF8

    $currentStage = 'sign-sys'
    Invoke-NativeStage $currentStage $SignToolPath @('sign', '/s', 'My', '/sha1', $thumbprint, '/fd', 'SHA256', (Join-Path $outputDirectory 'SayAllHidFilter.sys'))
    $currentStage = 'regenerate-catalog'
    Invoke-NativeStage $currentStage $Inf2CatPath @(('/driver:' + $outputDirectory), '/os:10_GE_X64')
    if (-not (Test-Path -LiteralPath (Join-Path $outputDirectory 'SayAllHidFilter.cat') -PathType Leaf)) { throw 'Inf2Cat did not create the fixed catalog.' }
    $currentStage = 'sign-cat'
    Invoke-NativeStage $currentStage $SignToolPath @('sign', '/s', 'My', '/sha1', $thumbprint, '/fd', 'SHA256', (Join-Path $outputDirectory 'SayAllHidFilter.cat'))

    $currentStage = 'record-manifest'
    $signerMetadata = @(
        (Read-SignerMetadata (Join-Path $outputDirectory 'SayAllHidFilter.sys') $thumbprint),
        (Read-SignerMetadata (Join-Path $outputDirectory 'SayAllHidFilter.cat') $thumbprint)
    )
    $finalNames = $payloadNames + @('LICENSE', $certificateFile)
    $actualOutputNames = @(Get-ChildItem -LiteralPath $outputDirectory -Force | ForEach-Object {
        Assert-NoReparsePath $_.FullName
        if ($_.PSIsContainer) { throw 'Unexpected directory in signed package.' }
        $_.Name
    })
    if (@(Compare-Object $finalNames $actualOutputNames).Count -ne 0) { throw 'Signed package contains an unexpected file set.' }
    $files = @($finalNames | ForEach-Object {
        $file = Get-Item -LiteralPath (Join-Path $outputDirectory $_)
        [ordered]@{ name = $_; sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash; bytes = $file.Length }
    })
    $manifest = [ordered]@{
        schema = 1; run_id = $runId; prepared_at_utc = [DateTime]::UtcNow.ToString('o')
        signing = 'local_test'; source_revision = $receipt.source_revision; working_tree_revision = $head
        reviewed_commit = $receipt.reviewed_commit; source_hashes_match_reviewed_commit = $true
        source_files = $receipt.source_files; build_receipt_sha256 = (Get-FileHash -LiteralPath $ReceiptPath -Algorithm SHA256).Hash
        hardware_id = $hardwareId; service_name = 'SayAllHidFilter'; extension_id = $extensionId; catalog_os = '10_GE_X64'
        certificate = [ordered]@{ thumbprint = $thumbprint; public_certificate_file = $certificateFile; store = 'CurrentUser/My'; private_key_exportable = $false; not_after_utc = $certificate.NotAfter.ToUniversalTime().ToString('o') }
        files = $files; signer_metadata = $signerMetadata
        verification = [ordered]@{ signer_thumbprints_match = $true; trust_imported = $false; signtool_verify_pa = 'deferred_until_trust_import'; catalog_membership = 'deferred_until_trust_import' }
        installed = $false; boot_configuration_changed = $false; hardware_validation = 'deferred'
        stages = @($stages.ToArray())
    }
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $outputDirectory 'signing-manifest.json') -Encoding UTF8
    Write-Stage $currentStage 0 0
    Write-Host 'driver_package=prepared_local_test_signed trust_imported=false installed=false boot_configuration_changed=false signature_trust_verification=deferred'
} catch {
    if (Test-Path -LiteralPath $logDirectory -PathType Container) {
        # Detailed failure text stays in this ignored local evidence directory;
        # console and production diagnostics never receive paths or certificate data.
        $failure = [ordered]@{ stage = $currentStage; reason = $_.Exception.Message; hresult = $_.Exception.HResult; certificate_thumbprint = $null; incomplete_output_preserved = $true; trust_imported = $false; installed = $false }
        if ($null -ne $certificate) { $failure.certificate_thumbprint = $certificate.Thumbprint }
        $failure | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $logDirectory 'failure.json') -Encoding UTF8
        Write-Stage ($currentStage + '-failure') 1 0
    }
    Write-Error ('RC003 signing preparation failed at stage ' + $currentStage + '. Local artifacts are preserved; no trust, installation or boot settings were changed.')
    exit 1
}
