param(
    [string]$ToolchainScript,
    [switch]$SkipTests,
    [switch]$Installer,
    [switch]$SkipHelperBuild,
    [ValidateNotNullOrEmpty()]
    [string]$BuildChannel = 'local',
    [string]$ReleaseTag,
    [switch]$CreateUpdaterArtifacts
)
$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path -Parent $PSScriptRoot
if ($ToolchainScript) {
    if (-not (Test-Path -LiteralPath $ToolchainScript -PathType Leaf)) { throw '找不到工具链激活脚本。' }
    . $ToolchainScript
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw '需要 Rust MSVC 工具链；请先加载工具链环境。' }
# Bound concurrent compilations/tests on ordinary 16 GB Windows machines.
if (-not $env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS = '2' }
$env:SAYALL_BUILD_CHANNEL = $BuildChannel
# Do not let a previous release build's tag leak into an ordinary local build.
$env:SAYALL_RELEASE_TAG = if ([string]::IsNullOrWhiteSpace($ReleaseTag)) { $null } else { $ReleaseTag }
Push-Location $taskRepo
try {
    if (-not $SkipHelperBuild) {
        & (Join-Path $PSScriptRoot 'build-rc003-helper.ps1')
        if (-not $?) { throw '三键增强 Helper 构建失败。' }
    }
    $helperBundle = Join-Path $taskRepo 'target/rc003-helper/SayAllKeyHelper'
    if (-not (Test-Path -LiteralPath (Join-Path $helperBundle 'manifest.json'))) {
        throw '三键增强组件清单缺失，请先构建 Helper。'
    }
    # Build the same complete payload into the local executable layout and installer.
    $localResources = [IO.Path]::GetFullPath((Join-Path $taskRepo 'target/release/rc003-helper'))
    $releaseRoot = [IO.Path]::GetFullPath((Join-Path $taskRepo 'target/release')).TrimEnd('\') + '\'
    if (-not $localResources.StartsWith($releaseRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw '增强组件输出目录越界。'
    }
    if (Test-Path -LiteralPath $localResources) {
        if ((Get-Item -LiteralPath $localResources).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw '增强组件输出目录不能是链接。' }
        Add-Type -AssemblyName Microsoft.VisualBasic
        [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($localResources,
            [Microsoft.VisualBasic.FileIO.UIOption]::OnlyErrorDialogs,
            [Microsoft.VisualBasic.FileIO.RecycleOption]::SendToRecycleBin)
    }
    New-Item -ItemType Directory -Path $localResources -Force | Out-Null
    Get-ChildItem -LiteralPath $helperBundle -Force | Copy-Item -Destination $localResources -Recurse -Force
    $manifest = Get-Content -LiteralPath (Join-Path $localResources 'manifest.json') -Raw | ConvertFrom-Json
    foreach ($entry in $manifest.files) {
        if ((Get-FileHash -LiteralPath (Join-Path $localResources $entry.path) -Algorithm SHA256).Hash -ne $entry.sha256) {
            throw '增强组件复制后校验失败。'
        }
    }
    if (@(Get-ChildItem -LiteralPath $localResources -Recurse -File).Count -ne $manifest.files.Count + 1) {
        throw '增强组件目录包含清单外的文件。'
    }
    $bundleConfig = Join-Path $taskRepo 'target/rc003-helper/tauri-local-helper.json'
    $resourceMap = @{}
    # Tauri resolves paths from src-tauri. The payload is already verified in its
    # final resource directory, so its build step need not copy each DLL again.
    $resourceMap['../target/release/rc003-helper/'] = 'rc003-helper/'
    $bundleOverride = @{ resources = $resourceMap }
    if ($CreateUpdaterArtifacts) { $bundleOverride.createUpdaterArtifacts = $true }
    @{ bundle = $bundleOverride } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $bundleConfig -Encoding utf8
    if (-not (Test-Path -LiteralPath 'node_modules/.bin/vite.cmd')) {
        & npm.cmd exec --yes --package=pnpm@10.15.0 -- pnpm install --frozen-lockfile
        if ($LASTEXITCODE -ne 0) { throw '前端依赖准备失败。' }
    }
    & npm.cmd run build
    if ($LASTEXITCODE -ne 0) { throw '前端编译失败。' }
    if (-not $SkipTests) {
        & npm.cmd test -- --maxWorkers=2 --minWorkers=1
        if ($LASTEXITCODE -ne 0) { throw '前端测试失败。' }
        & cargo test --workspace --lib --locked
        if ($LASTEXITCODE -ne 0) { throw 'Rust 测试失败。' }
        & cargo fmt --all -- --check
        if ($LASTEXITCODE -ne 0) { throw 'Rust 格式检查失败。' }
    }
    if ($Installer) {
        & npm.cmd run tauri -- build --bundles nsis --config $bundleConfig
    } else {
        & npm.cmd run tauri -- build --no-bundle --config $bundleConfig
    }
    if ($LASTEXITCODE -ne 0) { throw 'Windows 程序编译失败。' }
    Write-Host (Join-Path $taskRepo 'target/release/remote-coding.exe')
} finally {
    Pop-Location
}
