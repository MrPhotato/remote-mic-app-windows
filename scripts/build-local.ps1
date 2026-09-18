param(
    [string]$ToolchainScript,
    [switch]$SkipTests,
    [switch]$Installer
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
$env:SAYALL_BUILD_CHANNEL = 'local'
Push-Location $taskRepo
try {
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
        & npm.cmd run tauri -- build --bundles nsis
    } else {
        & npm.cmd run tauri -- build --no-bundle
    }
    if ($LASTEXITCODE -ne 0) { throw 'Windows 程序编译失败。' }
    Write-Host (Join-Path $taskRepo 'target/release/remote-coding.exe')
} finally {
    Pop-Location
}
