param([Parameter(Mandatory = $true)][string]$MakensisPath)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$taskRepo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$taskFixture = Join-Path $taskRepo ('target/setup-forwarding-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskFixture | Out-Null
$taskSource = @'
using System;
using System.IO;
using System.Diagnostics;
using System.Text;
using System.Threading;
class Fixture {
    static int Main(string[] args) {
        var receipt = Environment.GetEnvironmentVariable("SAYALL_BOOTSTRAP_TEST_RECEIPT");
        if (String.IsNullOrEmpty(receipt)) return 99;
        File.WriteAllLines(receipt, new[] {
            Path.GetFileName(Process.GetCurrentProcess().MainModule.FileName),
            Environment.CommandLine,
            String.Join("\u001f", args)
        }, new UTF8Encoding(false));
        Thread.Sleep(150);
        return Int32.Parse(Environment.GetEnvironmentVariable("SAYALL_BOOTSTRAP_TEST_EXIT"));
    }
}
'@
$taskFixtureSource = Join-Path $taskFixture 'fixture.cs'
$taskSource | Set-Content -LiteralPath $taskFixtureSource -Encoding utf8
$taskPayload = Join-Path $taskFixture 'inert-payload.exe'
$taskCompiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
& $taskCompiler /nologo /target:winexe "/out:$taskPayload" $taskFixtureSource *> (Join-Path $taskFixture 'fixture-build.log')
if ($LASTEXITCODE -ne 0) { throw "测试程序编译失败：$taskFixture/fixture-build.log" }
$taskInstaller = Join-Path $taskFixture 'inert-wrapper.exe'
function ConvertTo-NsisString([string]$Value) { $Value.Replace('$', '$$').Replace('"', '$\"') }
$taskDefines = [ordered]@{
    SETUP_VERSION = '0.0.0'
    SETUP_OUTPUT = $taskInstaller
    SETUP_PAYLOAD = $taskPayload
    SETUP_HOST = $taskPayload
    SETUP_ICON = Join-Path $taskRepo 'src-tauri/icons/icon.ico'
    SETUP_WEBVIEW_BOOTSTRAPPER = $taskPayload
    SAYALL_BOOTSTRAP_LOG_DIR = $taskFixture
}
$taskNsi = @($taskDefines.GetEnumerator() | ForEach-Object { '!define ' + $_.Key + ' "' + (ConvertTo-NsisString $_.Value) + '"' })
$taskNsi += '!include "' + (ConvertTo-NsisString (Join-Path $PSScriptRoot 'bootstrap.nsi')) + '"'
$taskNsiPath = Join-Path $taskFixture 'compile.nsi'
[IO.File]::WriteAllLines($taskNsiPath, $taskNsi, [Text.UTF8Encoding]::new($false))
& $MakensisPath /V2 $taskNsiPath *> (Join-Path $taskFixture 'wrapper-build.log')
if ($LASTEXITCODE -ne 0) { throw "安装器测试编译失败：$taskFixture/wrapper-build.log" }

$taskCases = @(
    @{ name = 'passive-update-restart-directory'; raw = '/P /UPDATE /R /D=C:\SayAll Test\无线麦'; exit_code = 0 },
    @{ name = 'silent-cancel-result'; raw = '/S /D=D:\Tools\SayAll'; exit_code = 1602 },
    @{ name = 'upgrade-timeout-result'; raw = '/UPDATE /P /R'; exit_code = 1640 },
    @{ name = 'restart-only'; raw = '/R'; exit_code = 0 },
    @{ name = 'directory-only'; raw = '/D=C:\Users\example\My Apps\SayAll'; exit_code = 0 },
    @{ name = 'unknown-option'; raw = '/PROFILE="C:\A /P Folder"'; exit_code = 23 },
    @{ name = 'quoted-unknown-value'; raw = '/CUSTOM "contains /S and /P" /D=C:\目录\保留 空格'; exit_code = 0 },
    @{ name = 'multiple-spaces-and-tab'; raw = "/P`t  /UPDATE   /R /D=C:\SayAll  Double Space"; exit_code = 0 },
    @{ name = 'leading-and-trailing-spaces'; raw = '   /P /R /D=C:\SayAll   '; exit_code = 0 },
    @{ name = 'backslash-and-dollar'; raw = '/S /D=C:\test $literal\目录\'; exit_code = 0 }
)

# Exercise the actual GUI dispatch only when the real prerequisite is present.
# The fake host is a hidden inert executable; no app, mic, Bluetooth or UI is used.
$taskRuntimePresent = $false
foreach ($taskHive in @([Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryHive]::CurrentUser)) {
    $taskBase = [Microsoft.Win32.RegistryKey]::OpenBaseKey($taskHive, [Microsoft.Win32.RegistryView]::Registry32)
    try {
        $taskKey = $taskBase.OpenSubKey('Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}')
        if ($taskKey) {
            try {
                $taskVersion = [string]$taskKey.GetValue('pv')
                if ($taskVersion -and $taskVersion -ne '0.0.0.0') { $taskRuntimePresent = $true }
            } finally { $taskKey.Dispose() }
        }
    } finally { $taskBase.Dispose() }
}
if ($taskRuntimePresent) {
    $taskCases += @{ name = 'no-args-gui-success'; raw = ''; exit_code = 0; gui = $true }
    $taskCases += @{ name = 'no-args-gui-cancel'; raw = ''; exit_code = 1602; gui = $true }
}
$taskResults = @()
foreach ($taskCase in $taskCases) {
    $taskReceipt = Join-Path $taskFixture ($taskCase.name + '.txt')
    $taskInfo = [Diagnostics.ProcessStartInfo]::new()
    $taskInfo.FileName = $taskInstaller
    $taskInfo.Arguments = $taskCase.raw
    $taskInfo.UseShellExecute = $false
    $taskInfo.CreateNoWindow = $true
    $taskInfo.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    $taskInfo.EnvironmentVariables['SAYALL_BOOTSTRAP_TEST_RECEIPT'] = $taskReceipt
    $taskInfo.EnvironmentVariables['SAYALL_BOOTSTRAP_TEST_EXIT'] = [string]$taskCase.exit_code
    $taskWatch = [Diagnostics.Stopwatch]::StartNew()
    $taskProcess = [Diagnostics.Process]::Start($taskInfo)
    if (-not $taskProcess.WaitForExit(30000)) { throw "转发测试超时：$($taskCase.name)；未强杀进程。" }
    if ($taskProcess.ExitCode -ne $taskCase.exit_code) { throw "退出码不符：$($taskCase.name)，收到 $($taskProcess.ExitCode)" }
    if (-not (Test-Path -LiteralPath $taskReceipt -PathType Leaf)) { throw "未收到参数收据：$($taskCase.name)" }
    $taskLines = [IO.File]::ReadAllLines($taskReceipt)
    $taskExpectedName = if ($taskCase.ContainsKey('gui')) { 'sayall-setup.exe' } else { 'sayall-payload.exe' }
    if ($taskLines[0] -ne $taskExpectedName) { throw "分派目标不符：$($taskCase.name)" }
    $taskReceived = [regex]::Replace($taskLines[1], '^"[^"]+"\s?', '')
    if ($taskReceived -cne $taskCase.raw) { throw "原始参数被改变：$($taskCase.name)" }
    if ($taskWatch.ElapsedMilliseconds -lt 150) { throw "包装器未等待子进程：$($taskCase.name)" }
    $taskResults += [ordered]@{ case = $taskCase.name; result = 'passed'; exit_code = $taskProcess.ExitCode; elapsed_ms = $taskWatch.ElapsedMilliseconds }
    $taskProcess.Dispose()
}
if (-not $taskRuntimePresent) { $taskResults += [ordered]@{ case = 'no-args-gui'; result = 'deferred'; reason = 'WebView2 is not installed; fixture never installs it' } }
$taskLog = [IO.File]::ReadAllLines((Join-Path $taskFixture 'setup-bootstrap.log'))
if (@($taskLog | Where-Object { $_ -match '^event=bootstrap_complete exit_code=' }).Count -ne $taskCases.Count) {
    throw '安装诊断日志未保留全部执行结果。'
}
if (@($taskLog | Where-Object { $_ -match '^event=bootstrap_dispatch route=' }).Count -ne $taskCases.Count) {
    throw '安装诊断日志未保留全部分派记录。'
}
$taskOverflowReceipt = Join-Path $taskFixture 'overflow-must-not-launch.txt'
$taskOverflow = [Diagnostics.ProcessStartInfo]::new()
$taskOverflow.FileName = $taskInstaller
$taskOverflow.Arguments = '/S /D=C:\' + ('a' * 10000)
$taskOverflow.UseShellExecute = $false
$taskOverflow.CreateNoWindow = $true
$taskOverflow.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
$taskOverflow.EnvironmentVariables['SAYALL_BOOTSTRAP_TEST_RECEIPT'] = $taskOverflowReceipt
$taskOverflow.EnvironmentVariables['SAYALL_BOOTSTRAP_TEST_EXIT'] = '0'
$taskOverflowProcess = [Diagnostics.Process]::Start($taskOverflow)
if (-not $taskOverflowProcess.WaitForExit(30000)) { throw '超长参数测试超时；未强杀进程。' }
if ($taskOverflowProcess.ExitCode -ne 1603 -or (Test-Path -LiteralPath $taskOverflowReceipt)) {
    throw '超长参数未在启动子安装器之前拒绝。'
}
$taskOverflowProcess.Dispose()
$taskResults += [ordered]@{ case = 'oversized-command-line'; result = 'passed'; exit_code = 1603 }
$taskResults | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskFixture 'results.json') -Encoding utf8
Write-Host "安装参数转发验证：$($taskCases.Count + 1) 项通过。收据：$taskFixture/results.json"
