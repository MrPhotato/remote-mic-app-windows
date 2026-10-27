param([Parameter(Mandatory = $true)][string]$MakensisPath)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$taskRepo = Split-Path -Parent $PSScriptRoot
$taskId = [Guid]::NewGuid().ToString('N')
$taskRoot = Join-Path $taskRepo ('target/setup-capture-fixture-' + $taskId)
$taskStage = Join-Path $taskRepo ('target/setup-build/0.0.1-' + $taskId)
New-Item -ItemType Directory -Path $taskRoot, $taskStage -Force | Out-Null
$taskReceipt = Join-Path $taskStage 'capture-receipt.json'
$taskSource = Join-Path $taskRoot 'inert-app.exe'
# Neither version of this payload is executable. The installer only extracts
# bytes inside this unique fixture directory; it has no registry or app actions.
$taskOriginal = [Text.Encoding]::UTF8.GetBytes('INERT __TAURI_BUNDLE_TYPE_VAR_UNK source bytes')
$taskPackaged = [Text.Encoding]::UTF8.GetBytes('INERT __TAURI_BUNDLE_TYPE_VAR_NSS source bytes; extra signing fixture bytes')
[IO.File]::WriteAllBytes($taskSource, $taskPackaged)
$taskHooks = Get-Content -LiteralPath (Join-Path $taskRepo 'src-tauri/windows/installer-hooks.nsh') -Raw
$taskMacro = [regex]::Match($taskHooks, '(?s)!macro SayAllCaptureInstalledApp\b.*?!macroend').Value
if (-not $taskMacro -or $taskHooks -notmatch '(?s)!macro NSIS_HOOK_PREINSTALL\s+!insertmacro SayAllCaptureInstalledApp') {
    throw 'Production preinstall capture hook is missing'
}
function ConvertTo-NsisString([string]$Value) { $Value.Replace('$', '$$').Replace('"', '$\"') }
function Compile-CaptureFixture([string]$Name, [bool]$ShouldPass) {
    $output = Join-Path $taskRoot ($Name + '.exe')
    $text = @'
Unicode true
SilentInstall silent
RequestExecutionLevel user
Name "SayAll application capture fixture"
OutFile "@OUTPUT@"
!define VERSION "0.0.1"
!define MAINBINARYSRCPATH "@SOURCE@"
@MACRO@
Section
  !insertmacro SayAllCaptureInstalledApp
  SetOutPath "$INSTDIR"
  File /oname=remote-coding.exe "${MAINBINARYSRCPATH}"
SectionEnd
'@
    $text = $text.Replace('@OUTPUT@', (ConvertTo-NsisString $output)).Replace('@SOURCE@', (ConvertTo-NsisString $taskSource)).Replace('@MACRO@', $taskMacro)
    $scriptPath = Join-Path $taskRoot ($Name + '.nsi')
    [IO.File]::WriteAllText($scriptPath, $text, [Text.UTF8Encoding]::new($false))
    & $MakensisPath /V2 $scriptPath *> (Join-Path $taskRoot ($Name + '.log'))
    if (($LASTEXITCODE -eq 0) -ne $ShouldPass) { throw "Unexpected capture fixture compilation result: $Name" }
    return $output
}
$taskOldScript = $env:SAYALL_SETUP_CAPTURE_SCRIPT
$taskOldReceipt = $env:SAYALL_SETUP_CAPTURE_RECEIPT
$taskOldEnabled = $env:SAYALL_SETUP_CAPTURE_ENABLED
try {
    $env:SAYALL_SETUP_CAPTURE_SCRIPT = Join-Path $PSScriptRoot 'capture-setup-app.ps1'
    $env:SAYALL_SETUP_CAPTURE_RECEIPT = $taskReceipt
    $env:SAYALL_SETUP_CAPTURE_ENABLED = '1'
    $taskInstaller = Compile-CaptureFixture 'captured' $true
    # Reusing a prior capture must stop compilation, rather than silently
    # authenticate an installer using an earlier build's application hash.
    Compile-CaptureFixture 'duplicate-capture' $false | Out-Null
    # Tauri restores this source after the NSIS compiler has consumed it.
    [IO.File]::WriteAllBytes($taskSource, $taskOriginal)
    $env:SAYALL_SETUP_CAPTURE_SCRIPT = $null
    Compile-CaptureFixture 'missing-script' $false | Out-Null
    $env:SAYALL_SETUP_CAPTURE_SCRIPT = Join-Path $PSScriptRoot 'capture-setup-app.ps1'
    $env:SAYALL_SETUP_CAPTURE_RECEIPT = $null
    Compile-CaptureFixture 'missing-receipt-path' $false | Out-Null
    $env:SAYALL_SETUP_CAPTURE_SCRIPT = $null
    $env:SAYALL_SETUP_CAPTURE_ENABLED = $null
    Compile-CaptureFixture 'capture-disabled' $true | Out-Null
} finally {
    [IO.File]::WriteAllBytes($taskSource, $taskOriginal)
    $env:SAYALL_SETUP_CAPTURE_SCRIPT = $taskOldScript
    $env:SAYALL_SETUP_CAPTURE_RECEIPT = $taskOldReceipt
    $env:SAYALL_SETUP_CAPTURE_ENABLED = $taskOldEnabled
}
$taskCapture = & (Join-Path $PSScriptRoot 'read-setup-capture.ps1') -ReceiptPath $taskReceipt -Version '0.0.1'
$taskBuildHash = (Get-FileHash -LiteralPath $taskSource -Algorithm SHA256).Hash.ToLowerInvariant()
if ($taskBuildHash -ceq $taskCapture.installed_app_sha256) { throw 'Capture fixture failed to preserve distinct packaged bytes' }
$taskInstall = Join-Path $taskRoot '中文目录 with spaces'
$taskProcess = Start-Process -FilePath $taskInstaller -ArgumentList ('/S /D=' + $taskInstall) -WindowStyle Hidden -PassThru
if (-not $taskProcess.WaitForExit(15000)) { throw 'Inert extraction fixture timed out' }
if ($taskProcess.ExitCode -ne 0) { throw 'Inert extraction fixture failed' }
$taskInstalledHash = (Get-FileHash -LiteralPath (Join-Path $taskInstall 'remote-coding.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
if ($taskInstalledHash -cne $taskCapture.installed_app_sha256) { throw 'NSIS extracted bytes differ from the compile-time capture' }
$taskResult = [ordered]@{
    check = 'setup_application_capture'
    result = 'passed'
    compile_cases = 5
    packaged_bytes_match_capture = $true
    restored_build_bytes_differ = $true
    extra_signing_bytes_preserved = $true
    build_app_sha256 = $taskBuildHash
    installed_app_sha256 = $taskInstalledHash
    application_executed = $false
    production_state_modified = $false
}
$taskResult | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskRoot 'result.json') -Encoding utf8
Write-Host 'Setup capture: 5 compile cases and real NSIS byte extraction passed.'
