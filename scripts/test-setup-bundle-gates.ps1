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
    $buildId = [Guid]::NewGuid().ToString('N')
    $captureStage = Join-Path $fixture ('target/setup-build/0.0.1-' + $buildId)
    New-Item -ItemType Directory -Path $captureStage -Force | Out-Null
    $capturedApp = Join-Path $captureStage 'installed-app.exe'
    $captureReceiptPath = Join-Path $captureStage 'capture-receipt.json'
    $payload = Join-Path $captureStage 'sayall-payload.exe'
    # Plain inert files make it impossible for this test to install or run apps.
    Write-FixtureText $installer 'final wrapper fixture; never executable'
    Write-FixtureText $app 'application fixture; never executable'
    Write-FixtureText $hostBinary 'setup host fixture; never executable'
    Write-FixtureText $capturedApp 'packaged application fixture; patched and signed bytes differ from build output'
    Write-FixtureText $payload 'inner NSIS fixture; never executable'
    $capture = [ordered]@{
        schema_version = 1
        version = '0.0.1'
        build_id = $buildId
        installed_app_sha256 = (Get-FileHash -LiteralPath $capturedApp).Hash.ToLowerInvariant()
        installed_app_size_bytes = (Get-Item -LiteralPath $capturedApp).Length
    }
    Write-FixtureText $captureReceiptPath ($capture | ConvertTo-Json)
    $receipt = [ordered]@{
        schema_version = 2
        version = '0.0.1'
        build_id = $buildId
        installer_sha256 = (Get-FileHash -LiteralPath $installer).Hash.ToLowerInvariant()
        payload_sha256 = (Get-FileHash -LiteralPath $payload).Hash.ToLowerInvariant()
        build_app_sha256 = (Get-FileHash -LiteralPath $app).Hash.ToLowerInvariant()
        installed_app_sha256 = $capture.installed_app_sha256
        capture_receipt_sha256 = (Get-FileHash -LiteralPath $captureReceiptPath).Hash.ToLowerInvariant()
        host_sha256 = (Get-FileHash -LiteralPath $hostBinary).Hash.ToLowerInvariant()
        webview_bootstrapper_sha256 = 'b' * 64
        updater_signed = $false
    }
    [pscustomobject]@{
        root=$fixture; installer=$installer; app=$app; host_binary=$hostBinary
        receipt_path=(Join-Path $release 'setup-build-receipt.json'); receipt=$receipt
        captured_app=$capturedApp; capture_path=$captureReceiptPath; capture=$capture; payload=$payload
    }
}
$taskCases = @(
    @{ name='unsigned-local-wrapper'; accept=$true },
    @{ name='signed-wrapper-receipt'; accept=$true; signed=$true; require_signed=$true },
    @{ name='missing-receipt'; error='receipt is missing'; omit_receipt=$true },
    @{ name='receipt-is-array'; error='must be an object'; array=$true },
    @{ name='old-ambiguous-app-schema'; error='schema is unsupported'; change={param($f) $f.receipt.schema_version=1} },
    @{ name='unsupported-schema'; error='schema is unsupported'; change={param($f) $f.receipt.schema_version=3} },
    @{ name='string-schema'; error='schema is unsupported'; change={param($f) $f.receipt.schema_version='2'} },
    @{ name='stale-version'; error='version does not match'; change={param($f) $f.receipt.version='0.0.0'} },
    @{ name='missing-host-hash'; error='missing host_sha256'; change={param($f) $f.receipt.Remove('host_sha256')} },
    @{ name='invalid-payload-hash'; error='hash is invalid: payload_sha256'; change={param($f) $f.receipt.payload_sha256='not-a-hash'} },
    @{ name='inner-payload-declared-final'; error='inner payload as the final'; change={param($f) $f.receipt.payload_sha256=$f.receipt.installer_sha256} },
    @{ name='inner-replaced-final-wrapper'; error='differs from the receipt: installer_sha256'; change={param($f) Write-FixtureText $f.installer 'inner NSIS substituted after wrapper receipt'} },
    @{ name='app-rebuilt-after-wrapper'; error='differs from the receipt: build_app_sha256'; change={param($f) Write-FixtureText $f.app 'different app bytes'} },
    @{ name='host-rebuilt-after-wrapper'; error='differs from the receipt: host_sha256'; change={param($f) Write-FixtureText $f.host_binary 'different host bytes'} },
    @{ name='missing-host'; error='file is missing: host_sha256'; change={param($f) Move-Item -LiteralPath $f.host_binary -Destination ($f.host_binary+'.held')} },
    @{ name='string-signed-flag'; error='must be a boolean'; change={param($f) $f.receipt.updater_signed='true'} },
    @{ name='unsigned-blocked-from-release'; error='not signed for publication'; require_signed=$true },
    @{ name='signed-without-signature'; error='signature is missing or empty'; require_signed=$true; change={param($f) $f.receipt.updater_signed=$true} },
    @{ name='signed-with-empty-signature'; error='signature is missing or empty'; require_signed=$true; signed=$true; change={param($f) Write-FixtureText ($f.installer+'.sig') ' '} },
    @{ name='invalid-build-id'; error='build identity is invalid'; change={param($f) $f.receipt.build_id='../old-build'} },
    @{ name='missing-capture'; error='capture receipt is missing'; change={param($f) Move-Item -LiteralPath $f.capture_path -Destination ($f.capture_path+'.held')} },
    @{ name='capture-is-array'; error='capture receipt must be an object'; change={param($f) Write-FixtureText $f.capture_path ('['+($f.capture | ConvertTo-Json)+']')} },
    @{ name='capture-from-different-build'; error='version or build identity does not match'; change={param($f) $f.capture.build_id='0'*32; Write-FixtureText $f.capture_path ($f.capture | ConvertTo-Json)} },
    @{ name='capture-from-old-version'; error='version or build identity does not match'; change={param($f) $f.capture.version='0.0.0'; Write-FixtureText $f.capture_path ($f.capture | ConvertTo-Json)} },
    @{ name='capture-receipt-replaced'; error='capture differs from the final receipt'; change={param($f) $f.capture.extra='changed'; Write-FixtureText $f.capture_path ($f.capture | ConvertTo-Json)} },
    @{ name='captured-app-replaced-with-restored-build'; error='captured application differs'; change={param($f) Copy-Item -LiteralPath $f.app -Destination $f.captured_app -Force} },
    @{ name='capture-size-changed'; error='captured application differs'; change={param($f) $f.capture.installed_app_size_bytes++; Write-FixtureText $f.capture_path ($f.capture | ConvertTo-Json)} },
    @{ name='missing-captured-app'; error='captured application is missing'; change={param($f) Move-Item -LiteralPath $f.captured_app -Destination ($f.captured_app+'.held')} },
    @{ name='wrong-installed-app-hash'; error='capture differs from the final receipt'; change={param($f) $f.receipt.installed_app_sha256=$f.receipt.build_app_sha256} },
    @{ name='inner-payload-changed'; error='inner payload differs'; change={param($f) Write-FixtureText $f.payload 'different payload bytes'} },
    @{ name='missing-inner-payload'; error='inner payload is missing'; change={param($f) Move-Item -LiteralPath $f.payload -Destination ($f.payload+'.held')} },
    @{ name='unknown-local-fields-not-published'; accept=$true; change={param($f) $f.receipt.local_path='DO_NOT_COPY_LOCAL_FIELD'} },
    @{ name='wrong-installer-name'; error='identity does not match'; change={param($f) $newPath=Join-Path (Split-Path -Parent $f.installer) 'Remote Coding_0.0.0_x64-setup.exe'; Move-Item -LiteralPath $f.installer -Destination $newPath; $f.installer=$newPath} }
)
$taskPublicFields = @('schema_version','version','build_id','installer_sha256','payload_sha256','build_app_sha256','installed_app_sha256','capture_receipt_sha256','host_sha256','webview_bootstrapper_sha256','updater_signed')
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
