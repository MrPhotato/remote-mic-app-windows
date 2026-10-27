param(
    [Parameter(Mandatory = $true)][string]$InstallerPath,
    [Parameter(Mandatory = $true)][string]$CaptureReceiptPath,
    [string]$MakensisPath,
    [switch]$CreateUpdaterArtifacts
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$taskRepo = Split-Path -Parent $PSScriptRoot
$taskConfig = Get-Content -LiteralPath (Join-Path $taskRepo 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$taskVersion = [string]$taskConfig.version
if ($taskVersion -notmatch '^\d+\.\d+\.\d+$') { throw '安装器版本格式无效。' }
$taskCapture = & (Join-Path $PSScriptRoot 'read-setup-capture.ps1') -ReceiptPath $CaptureReceiptPath -Version $taskVersion
$taskInstaller = (Resolve-Path -LiteralPath $InstallerPath).Path
$taskBundleRoot = [IO.Path]::GetFullPath((Join-Path $taskRepo 'target/release/bundle/nsis')).TrimEnd('\') + '\'
if (-not $taskInstaller.StartsWith($taskBundleRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw '内部安装器必须位于本仓库的 NSIS 输出目录。'
}
$taskApp = Join-Path $taskRepo 'target/release/remote-coding.exe'
if (-not (Test-Path -LiteralPath $taskApp -PathType Leaf)) { throw '缺少已构建的无线麦主程序。' }
$taskInnerInfo = [Diagnostics.FileVersionInfo]::GetVersionInfo($taskInstaller)
if ($taskInnerInfo.ProductName -ne 'Remote Coding' -or $taskInnerInfo.FileVersion -ne $taskVersion) {
    throw '必须包装本次版本的原始 Tauri NSIS；不可重复包装或使用旧安装器。'
}
if ([Diagnostics.FileVersionInfo]::GetVersionInfo($taskApp).FileVersion -ne $taskVersion) {
    throw '主程序版本与安装器版本不一致。'
}
if ($CreateUpdaterArtifacts -and [string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY)) {
    throw '公开安装器必须提供正式更新签名密钥。'
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw '请先加载 Rust/MSVC 构建环境。' }

if (-not $MakensisPath) {
    $taskNsisCandidates = @()
    $taskNsisCommand = Get-Command makensis.exe -ErrorAction SilentlyContinue
    if ($taskNsisCommand) { $taskNsisCandidates += $taskNsisCommand.Source }
    foreach ($taskLocalRoot in @($env:LOCALAPPDATA, [Environment]::GetFolderPath('LocalApplicationData'), (Join-Path $env:USERPROFILE 'AppData/Local'))) {
        if ($taskLocalRoot) { $taskNsisCandidates += Join-Path $taskLocalRoot 'tauri/NSIS/makensis.exe' }
    }
    if (${env:ProgramFiles(x86)}) { $taskNsisCandidates += Join-Path ${env:ProgramFiles(x86)} 'NSIS/makensis.exe' }
    $MakensisPath = $taskNsisCandidates | Select-Object -Unique | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
}
if (-not $MakensisPath -or -not (Test-Path -LiteralPath $MakensisPath -PathType Leaf)) { throw '找不到 NSIS 编译器。' }
$MakensisPath = (Resolve-Path -LiteralPath $MakensisPath).Path
$taskBuildRoot = Join-Path $taskRepo 'target/setup-build'
$taskStage = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($CaptureReceiptPath))
$taskPayload = Join-Path $taskStage 'sayall-payload.exe'
if (Test-Path -LiteralPath $taskPayload) { throw '本次打包目录已使用；请重新构建内部安装器。' }
Copy-Item -LiteralPath $taskInstaller -Destination $taskPayload
$taskPayloadHash = (Get-FileHash -LiteralPath $taskPayload -Algorithm SHA256).Hash.ToLowerInvariant()
$taskBuildAppHash = (Get-FileHash -LiteralPath $taskApp -Algorithm SHA256).Hash.ToLowerInvariant()

# Keep the official bootstrapper in a build cache, not in source control. Verify
# its publisher every build, even when cached; never execute an unverified file.
$taskPrerequisites = Join-Path $taskBuildRoot 'prerequisites'
New-Item -ItemType Directory -Path $taskPrerequisites -Force | Out-Null
$taskWebView = Join-Path $taskPrerequisites 'MicrosoftEdgeWebview2Setup.exe'
if (-not (Test-Path -LiteralPath $taskWebView -PathType Leaf)) {
    $taskDownloadedWebView = Join-Path $taskStage 'MicrosoftEdgeWebview2Setup.download.exe'
    Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $taskDownloadedWebView
    $taskSignature = Get-AuthenticodeSignature -LiteralPath $taskDownloadedWebView
    if ($taskSignature.Status -ne 'Valid' -or $taskSignature.SignerCertificate.Subject -notmatch '(^|,\s*)O=Microsoft Corporation(,|$)') {
        throw 'WebView2 引导程序的 Microsoft 签名验证失败。已保留下载文件。'
    }
    Move-Item -LiteralPath $taskDownloadedWebView -Destination $taskWebView
}
$taskSignature = Get-AuthenticodeSignature -LiteralPath $taskWebView
if ($taskSignature.Status -ne 'Valid' -or $taskSignature.SignerCertificate.Subject -notmatch '(^|,\s*)O=Microsoft Corporation(,|$)') {
    throw '缓存的 WebView2 引导程序未通过 Microsoft 签名验证。'
}

$taskPreviousPayloadHash = $env:SAYALL_SETUP_PAYLOAD_SHA256
$taskPreviousAppHash = $env:SAYALL_SETUP_APP_SHA256
Push-Location $taskRepo
try {
    & npm.cmd run setup:build *> (Join-Path $taskStage 'frontend-build.log')
    if ($LASTEXITCODE -ne 0) { throw "安装界面构建失败，日志：$taskStage/frontend-build.log" }
    $env:SAYALL_SETUP_PAYLOAD_SHA256 = $taskPayloadHash
    $env:SAYALL_SETUP_APP_SHA256 = $taskCapture.installed_app_sha256
    & cargo build --release -p sayall-setup --locked *> (Join-Path $taskStage 'host-build.log')
    if ($LASTEXITCODE -ne 0) { throw "安装宿主构建失败，日志：$taskStage/host-build.log" }
    $taskHost = Join-Path $taskRepo 'target/release/sayall-setup.exe'
    if (-not (Test-Path -LiteralPath $taskHost -PathType Leaf)) { throw '安装宿主产物缺失。' }

    # Generate a tiny include file rather than interpolating paths into shell
    # command text. Escape NSIS string syntax, including dollar signs in paths.
    function ConvertTo-NsisString([string]$Value) { $Value.Replace('$', '$$').Replace('"', '$\"') }
    $taskOutput = Join-Path $taskStage ([IO.Path]::GetFileName($taskInstaller))
    $taskDefines = [ordered]@{
        SETUP_VERSION = $taskVersion
        SETUP_OUTPUT = $taskOutput
        SETUP_PAYLOAD = $taskPayload
        SETUP_HOST = $taskHost
        SETUP_ICON = Join-Path $taskRepo 'src-tauri/icons/icon.ico'
        SETUP_WEBVIEW_BOOTSTRAPPER = $taskWebView
    }
    $taskSource = @($taskDefines.GetEnumerator() | ForEach-Object { '!define ' + $_.Key + ' "' + (ConvertTo-NsisString $_.Value) + '"' })
    $taskSource += '!include "' + (ConvertTo-NsisString (Join-Path $taskRepo 'tools/setup-bootstrap/bootstrap.nsi')) + '"'
    $taskSourcePath = Join-Path $taskStage 'compile.nsi'
    [IO.File]::WriteAllLines($taskSourcePath, $taskSource, [Text.UTF8Encoding]::new($false))
    & $MakensisPath /V2 $taskSourcePath *> (Join-Path $taskStage 'nsis-build.log')
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $taskOutput -PathType Leaf)) {
        throw "安装器包装失败，日志：$taskStage/nsis-build.log"
    }

    if ($CreateUpdaterArtifacts) {
        # Key and password are read from the inherited environment. Never place
        # either value on a command line or retain signer output in build logs.
        $taskSigner = Join-Path $taskRepo 'node_modules/@tauri-apps/cli/tauri.js'
        $taskSignInfo = [Diagnostics.ProcessStartInfo]::new()
        $taskSignInfo.FileName = (Get-Command node.exe).Source
        $taskSignInfo.UseShellExecute = $false
        $taskSignInfo.CreateNoWindow = $true
        $taskSignInfo.RedirectStandardOutput = $true
        $taskSignInfo.RedirectStandardError = $true
        $taskSignInfo.RedirectStandardInput = $true
        $taskSignInfo.Arguments = '"' + $taskSigner + '" signer sign "' + $taskOutput + '"'
        # An explicit empty password supports the project's unencrypted release
        # key without allowing an unattended prompt when the env var is absent.
        $taskSignInfo.EnvironmentVariables['TAURI_SIGNING_PRIVATE_KEY_PASSWORD'] = [string]$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
        $taskSigning = [Diagnostics.Process]::Start($taskSignInfo)
        $taskSigning.StandardInput.Close()
        $taskStdout = $taskSigning.StandardOutput.ReadToEndAsync()
        $taskStderr = $taskSigning.StandardError.ReadToEndAsync()
        if (-not $taskSigning.WaitForExit(60000)) {
            # Stop only the signer process we just created. It owns no app or
            # device state; never leave an interactive credential prompt behind.
            $taskSigning.Kill()
            $taskSigning.WaitForExit()
            $taskSigning.Dispose()
            throw '最终安装器签名超时；未替换 NSIS 输出。'
        }
        $taskStdout.GetAwaiter().GetResult() | Out-Null
        $taskStderr.GetAwaiter().GetResult() | Out-Null
        if ($taskSigning.ExitCode -ne 0 -or -not (Test-Path -LiteralPath ($taskOutput + '.sig') -PathType Leaf)) {
            throw '最终安装器签名失败；未替换 NSIS 输出。'
        }
        $taskSigning.Dispose()
        $taskPublicKeyPath = Join-Path $taskStage 'release-signing.pub'
        [IO.File]::WriteAllText($taskPublicKeyPath, [string]$taskConfig.plugins.updater.pubkey, [Text.UTF8Encoding]::new($false))
        & cargo run --quiet --locked --manifest-path (Join-Path $taskRepo 'tools/release-signature-verify/Cargo.toml') -- $taskOutput ($taskOutput + '.sig') $taskPublicKeyPath *> (Join-Path $taskStage 'signature-verification.log')
        if ($LASTEXITCODE -ne 0) { throw '最终安装器未通过应用固定公钥的签名验证；未替换 NSIS 输出。' }
    }

    # Replace only this exact build's installer after all work succeeds. Preserve
    # the previous installer/signature outside the bundle directory for diagnosis.
    $taskPrevious = Join-Path $taskStage 'previous-output'
    New-Item -ItemType Directory -Path $taskPrevious | Out-Null
    Move-Item -LiteralPath $taskInstaller -Destination (Join-Path $taskPrevious ([IO.Path]::GetFileName($taskInstaller)))
    if (Test-Path -LiteralPath ($taskInstaller + '.sig')) {
        Move-Item -LiteralPath ($taskInstaller + '.sig') -Destination (Join-Path $taskPrevious ([IO.Path]::GetFileName($taskInstaller) + '.sig'))
    }
    Move-Item -LiteralPath $taskOutput -Destination $taskInstaller
    if ($CreateUpdaterArtifacts) { Move-Item -LiteralPath ($taskOutput + '.sig') -Destination ($taskInstaller + '.sig') }
    $taskReceipt = [ordered]@{
        schema_version = 2
        version = $taskVersion
        build_id = $taskCapture.build_id
        installer_sha256 = (Get-FileHash -LiteralPath $taskInstaller -Algorithm SHA256).Hash.ToLowerInvariant()
        payload_sha256 = $taskPayloadHash
        build_app_sha256 = $taskBuildAppHash
        installed_app_sha256 = $taskCapture.installed_app_sha256
        capture_receipt_sha256 = $taskCapture.capture_receipt_sha256
        host_sha256 = (Get-FileHash -LiteralPath $taskHost -Algorithm SHA256).Hash.ToLowerInvariant()
        webview_bootstrapper_sha256 = (Get-FileHash -LiteralPath $taskWebView -Algorithm SHA256).Hash.ToLowerInvariant()
        updater_signed = [bool]$CreateUpdaterArtifacts
    } | ConvertTo-Json
    $taskReceiptPath = Join-Path $taskStage 'build-receipt.json'
    [IO.File]::WriteAllText($taskReceiptPath, $taskReceipt, [Text.UTF8Encoding]::new($false))
    $taskStableReceipt = Join-Path $taskRepo 'target/release/setup-build-receipt.json'
    # Publish a complete, SHA-bound receipt with an atomic file replacement.
    $taskReceiptPending = Join-Path $taskRepo ('target/release/setup-build-receipt-' + [Guid]::NewGuid().ToString('N') + '.json')
    [IO.File]::WriteAllText($taskReceiptPending, $taskReceipt, [Text.UTF8Encoding]::new($false))
    if (Test-Path -LiteralPath $taskStableReceipt) {
        [IO.File]::Replace($taskReceiptPending, $taskStableReceipt, (Join-Path $taskPrevious 'setup-build-receipt.json'))
    } else {
        Move-Item -LiteralPath $taskReceiptPending -Destination $taskStableReceipt
    }
    Write-Host "安装界面已打包：$taskInstaller"
} finally {
    $env:SAYALL_SETUP_PAYLOAD_SHA256 = $taskPreviousPayloadHash
    $env:SAYALL_SETUP_APP_SHA256 = $taskPreviousAppHash
    Pop-Location
}
