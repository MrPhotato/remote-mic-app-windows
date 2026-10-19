param(
    [Parameter(Mandatory = $true)][string]$MakensisPath,
    [string]$UtilsPath = (Join-Path $PSScriptRoot '../target/release/nsis/x64/utils.nsh')
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$id = [Guid]::NewGuid().ToString('N')
$fixture = Join-Path $repo ('target/installer-exit-fixture-' + $id)
New-Item -ItemType Directory -Path $fixture | Out-Null
$binaryName = 'SayAllExitFixture-' + $id
$eventName = 'Local\SayAllExitFixture-' + $id
$stopName = $eventName + '-Stop'
$readyName = $eventName + '-Ready'
$source = @'
using System;
using System.Threading;
class Fixture {
    static void Main(string[] args) {
        using (var exit = new EventWaitHandle(false, EventResetMode.ManualReset, "@EVENT@"))
        using (var stop = new EventWaitHandle(false, EventResetMode.ManualReset, "@STOP@"))
        using (var ready = new EventWaitHandle(false, EventResetMode.ManualReset, "@READY@")) {
            ready.Set();
            if (args.Length > 0 && args[0] == "respond") exit.WaitOne(60000);
            else stop.WaitOne(60000);
        }
    }
}
'@
$source = $source.Replace('@EVENT@', $eventName.Replace('\','\\')).Replace('@STOP@', $stopName.Replace('\','\\')).Replace('@READY@', $readyName.Replace('\','\\'))
$sourcePath = Join-Path $fixture 'fixture.cs'
$source | Set-Content -LiteralPath $sourcePath -Encoding utf8
$app = Join-Path $fixture ($binaryName + '.exe')
$compiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
& $compiler /nologo /target:winexe "/out:$app" $sourcePath *> (Join-Path $fixture 'csharp.log')
if ($LASTEXITCODE -ne 0) { throw 'Exit fixture compilation failed' }
$nsi = @'
Unicode true
SilentInstall silent
!include MUI2.nsh
!include FileFunc.nsh
!include "Win\COM.nsh"
!include "Win\Propkey.nsh"
!include "@UTILS@"
!include "@HOOKS@"
!addplugindir "@PLUGINS@"
!undef SAYALL_GRACEFUL_EXIT_EVENT
!define SAYALL_GRACEFUL_EXIT_EVENT "@EVENT@"
!define MAINBINARYNAME "@BINARY@"
!define PRODUCTNAME "SayAll exit fixture"
Var AppStartMenuFolder
Name "SayAll exit fixture"
OutFile "@OUTPUT@"
RequestExecutionLevel user
Section
  !insertmacro SayAllRequestGracefulExit fixture
  ; Reaching this point models the installer being allowed to replace files.
  SetErrorLevel 0
SectionEnd
'@
$installer = Join-Path $fixture 'exit-fixture.exe'
$plugins = Join-Path (Split-Path $MakensisPath -Parent) 'Plugins/x86-unicode/additional'
if (!(Test-Path -LiteralPath $plugins)) { $plugins = Join-Path (Split-Path (Split-Path $MakensisPath -Parent) -Parent) 'Plugins/x86-unicode/additional' }
$nsi = $nsi.Replace('@UTILS@',(Resolve-Path $UtilsPath).Path).Replace('@HOOKS@',(Join-Path $repo 'src-tauri/windows/installer-hooks.nsh')).Replace('@PLUGINS@',$plugins).Replace('@EVENT@',$eventName).Replace('@BINARY@',$binaryName).Replace('@OUTPUT@',$installer)
$nsiPath = Join-Path $fixture 'exit-fixture.nsi'
$nsi | Set-Content -LiteralPath $nsiPath -Encoding utf8
& $MakensisPath /V2 $nsiPath *> (Join-Path $fixture 'compile.log')
if ($LASTEXITCODE -ne 0) { throw "NSIS fixture compilation failed: $fixture/compile.log" }
$results = @()
foreach ($mode in @('absent','respond','unresponsive')) {
    $process = $null
    $ready = [Threading.EventWaitHandle]::new($false, [Threading.EventResetMode]::ManualReset, $readyName)
    $stop = [Threading.EventWaitHandle]::new($false, [Threading.EventResetMode]::ManualReset, $stopName)
    try {
        if ($mode -ne 'absent') {
            $process = Start-Process -FilePath $app -ArgumentList $mode -WindowStyle Hidden -PassThru
            if (!$ready.WaitOne(5000)) { throw 'Fixture failed to signal readiness' }
        }
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $setup = Start-Process -FilePath $installer -ArgumentList '/S' -WindowStyle Hidden -PassThru
        if (!$setup.WaitForExit(30000)) { throw 'Installer fixture exceeded its bounded wait' }
        $expected = if ($mode -eq 'unresponsive') { 1640 } else { 0 }
        if ($setup.ExitCode -ne $expected) { throw "$mode installer exit $($setup.ExitCode), expected $expected" }
        if ($mode -eq 'unresponsive' -and $process.HasExited) { throw 'Unresponsive fixture was terminated' }
        if ($mode -eq 'respond' -and !$process.WaitForExit(1000)) { throw 'Cooperating fixture did not exit' }
        $results += [pscustomobject]@{ case=$mode; result='passed'; exit_code=$setup.ExitCode; elapsed_ms=$watch.ElapsedMilliseconds }
    } finally {
        $null = $stop.Set()
        if ($process -and !$process.WaitForExit(5000)) { throw 'Fixture remains alive; no process was forcibly terminated' }
        $ready.Dispose()
        $stop.Dispose()
    }
}
$results | ConvertTo-Json | Tee-Object (Join-Path $fixture 'results.json')
