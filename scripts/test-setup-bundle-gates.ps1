$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$taskRepo = Split-Path -Parent $PSScriptRoot
$taskRoot = Join-Path $taskRepo ('target/setup-bundle-gates-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskRoot | Out-Null
$taskGate = Join-Path $PSScriptRoot 'verify-setup-bundle.ps1'
$taskResults = @()
function Write-FixtureText([string]$Path, [string]$Text) {
    [IO.File]::WriteAllText($Path, $Text, [Text.UTF8Encoding]::new($false))
}
function New-BundleFixture([string]$Name) {
    $fixture = Join-Path $taskRoot $Name
    $release = Join-Path $fixture 'target/release'
    $bundle = Join-Path $release 'bundle/nsis'
    New-Item -ItemType Directory -Path $bundle, (Join-Path $fixture 'src-tauri') -Force | Out-Null
    Write-FixtureText (Join-Path $fixture 'src-tauri/tauri.conf.json') '{"version":"0.0.1"}'
    $installer = Join-Path $bundle 'Remote Coding_0.0.1_x64-setup.exe'
    $app = Join-Path $release 'remote-coding.exe'
    $hostBinary = Join-Path $release 'sayall-setup.exe'
    # Plain inert files make it impossible for this test to install or run apps.
    Write-FixtureText $installer 'final wrapper fixture; never executable'
    Write-FixtureText $app 'application fixture; never executable'
    Write-FixtureText $hostBinary 'setup host fixture; never executable'
    $receipt = [ordered]@{
        schema_version = 1
        version = '0.0.1'
        installer_sha256 = (Get-FileHash -LiteralPath $installer).Hash.ToLowerInvariant()
        payload_sha256 = 'a' * 64
        app_sha256 = (Get-FileHash -LiteralPath $app).Hash.ToLowerInvariant()
        host_sha256 = (Get-FileHash -LiteralPath $hostBinary).Hash.ToLowerInvariant()
        webview_bootstrapper_sha256 = 'b' * 64
        updater_signed = $false
    }
    [pscustomobject]@{ root=$fixture; installer=$installer; app=$app; host_binary=$hostBinary; receipt_path=(Join-Path $release 'setup-build-receipt.json'); receipt=$receipt }
}
$taskCases = @(
    @{ name='unsigned-local-wrapper'; accept=$true },
    @{ name='signed-wrapper-receipt'; accept=$true; signed=$true; require_signed=$true },
    @{ name='missing-receipt'; error='receipt is missing'; omit_receipt=$true },
    @{ name='receipt-is-array'; error='must be an object'; array=$true },
    @{ name='unsupported-schema'; error='schema is unsupported'; change={param($f) $f.receipt.schema_version=2} },
    @{ name='string-schema'; error='schema is unsupported'; change={param($f) $f.receipt.schema_version='1'} },
    @{ name='stale-version'; error='version does not match'; change={param($f) $f.receipt.version='0.0.0'} },
    @{ name='missing-host-hash'; error='missing host_sha256'; change={param($f) $f.receipt.Remove('host_sha256')} },
    @{ name='invalid-payload-hash'; error='hash is invalid: payload_sha256'; change={param($f) $f.receipt.payload_sha256='not-a-hash'} },
    @{ name='inner-payload-declared-final'; error='inner payload as the final'; change={param($f) $f.receipt.payload_sha256=$f.receipt.installer_sha256} },
    @{ name='inner-replaced-final-wrapper'; error='differs from the receipt: installer_sha256'; change={param($f) Write-FixtureText $f.installer 'inner NSIS substituted after wrapper receipt'} },
    @{ name='app-rebuilt-after-wrapper'; error='differs from the receipt: app_sha256'; change={param($f) Write-FixtureText $f.app 'different app bytes'} },
    @{ name='host-rebuilt-after-wrapper'; error='differs from the receipt: host_sha256'; change={param($f) Write-FixtureText $f.host_binary 'different host bytes'} },
    @{ name='missing-host'; error='file is missing: host_sha256'; change={param($f) Move-Item -LiteralPath $f.host_binary -Destination ($f.host_binary+'.held')} },
    @{ name='string-signed-flag'; error='must be a boolean'; change={param($f) $f.receipt.updater_signed='true'} },
    @{ name='unsigned-blocked-from-release'; error='not signed for publication'; require_signed=$true },
    @{ name='signed-without-signature'; error='signature is missing or empty'; require_signed=$true; change={param($f) $f.receipt.updater_signed=$true} },
    @{ name='signed-with-empty-signature'; error='signature is missing or empty'; require_signed=$true; signed=$true; change={param($f) Write-FixtureText ($f.installer+'.sig') ' '} },
    @{ name='unknown-local-fields-not-published'; accept=$true; change={param($f) $f.receipt.local_path='DO_NOT_COPY_LOCAL_FIELD'} },
    @{ name='wrong-installer-name'; error='identity does not match'; change={param($f) $newPath=Join-Path (Split-Path -Parent $f.installer) 'Remote Coding_0.0.0_x64-setup.exe'; Move-Item -LiteralPath $f.installer -Destination $newPath; $f.installer=$newPath} }
)
$taskPublicFields = @('schema_version','version','installer_sha256','payload_sha256','app_sha256','host_sha256','webview_bootstrapper_sha256','updater_signed')
foreach ($taskCase in $taskCases) {
    $taskFixture = New-BundleFixture $taskCase.name
    if ($taskCase.ContainsKey('signed')) {
        $taskFixture.receipt.updater_signed=$true
        Write-FixtureText ($taskFixture.installer+'.sig') 'Inert signature fixture: cryptographic verification is a separate gate.'
    }
    if ($taskCase.ContainsKey('change')) { & $taskCase.change $taskFixture }
    if (-not $taskCase.ContainsKey('omit_receipt')) {
        $taskReceiptJson = $taskFixture.receipt | ConvertTo-Json
        if ($taskCase.ContainsKey('array')) { $taskReceiptJson='['+$taskReceiptJson+']' }
        Write-FixtureText $taskFixture.receipt_path $taskReceiptJson
    }
    $taskAccepted=$false
    $taskError=''
    try {
        $taskChecked = & $taskGate -RepositoryRoot $taskFixture.root -InstallerPath $taskFixture.installer -RequireUpdaterSignature:$taskCase.ContainsKey('require_signed')
        $taskAccepted=$true
        if (@($taskChecked.PSObject.Properties.Name).Count -ne $taskPublicFields.Count -or
            @($taskChecked.PSObject.Properties.Name | Where-Object { $_ -notin $taskPublicFields }).Count) {
            throw 'Gate leaked unapproved metadata fields'
        }
        if ($taskChecked.installer_sha256 -cne $taskFixture.receipt.installer_sha256) { throw 'Gate returned different installer identity' }
    } catch { $taskError=$_.Exception.Message }
    $taskExpected=$taskCase.ContainsKey('accept') -and $taskCase.accept
    if ($taskAccepted -ne $taskExpected -or ($taskExpected -and $taskError) -or
        (-not $taskExpected -and $taskError -notlike ('*'+$taskCase.error+'*'))) {
        throw "Setup receipt fixture failed: $($taskCase.name); $taskError"
    }
    $taskResults += [ordered]@{case=$taskCase.name;result='passed';accepted=$taskAccepted}
}

# Exercise the real release entry point too: a stale/inner installer must be
# rejected before any manifest or staged release assets can be written.
$taskIntegration = New-BundleFixture 'release-entry-rejects-inner'
$taskIntegration.receipt.updater_signed=$true
Write-FixtureText ($taskIntegration.installer+'.sig') 'Inert signature; never published.'
Write-FixtureText $taskIntegration.receipt_path ($taskIntegration.receipt | ConvertTo-Json)
Write-FixtureText $taskIntegration.installer 'substituted inner NSIS bytes'
$taskRejected=$false
try { & (Join-Path $PSScriptRoot 'generate-updater-manifest.ps1') -Tag v0.0.1 -RepositoryRoot $taskIntegration.root }
catch { $taskRejected=$_.Exception.Message -like '*differs from the receipt: installer_sha256*' }
if (-not $taskRejected -or (Test-Path -LiteralPath (Join-Path $taskIntegration.root 'artifacts/windows-release'))) {
    throw 'Release entry point did not stop before staging the inner installer'
}
$taskResults += [ordered]@{case='release-entry-rejects-inner-before-staging';result='passed';accepted=$false}
$taskResults | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskRoot 'results.json') -Encoding utf8
Write-Host "Final setup receipt gates: $($taskResults.Count) cases passed."
