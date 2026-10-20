param([Parameter(Mandatory=$true)][string]$MakensisPath)
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
$directory=Join-Path $repo ('target/setup-shortcut-fixture-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $directory | Out-Null
$source=Get-Content -LiteralPath (Join-Path $repo 'src-tauri/windows/installer-hooks.nsh') -Raw
$read=[regex]::Match($source,'(?s)!macro SayAllReadDesktopChoice\b.*?!macroend').Value
$apply=[regex]::Match($source,'(?s)!macro SayAllApplyDesktopChoice\b.*?!macroend').Value
if(!$read -or !$apply){throw 'Production shortcut macros missing'}
$nsi=@'
Unicode true
SilentInstall silent
RequestExecutionLevel user
!include LogicLib.nsh
!include FileFunc.nsh
Name "SayAll shortcut choice fixture"
OutFile "@OUTPUT@"
Var NoShortcutMode
Var UpdateMode
Var SayAllDesktopChoice
Var SayAllOriginalShortcutMode
Var DesktopRequests
Var MenuRequests
@READ@
@APPLY@
Function CreateOrUpdateDesktopShortcut
  IntOp $DesktopRequests $DesktopRequests + 1
FunctionEnd
Function CreateOrUpdateStartMenuShortcut
  IntOp $MenuRequests $MenuRequests + 1
FunctionEnd
Section
  StrCpy $NoShortcutMode 0
  StrCpy $UpdateMode 1
  StrCpy $DesktopRequests 0
  StrCpy $MenuRequests 0
  !insertmacro SayAllReadDesktopChoice
  StrCpy $R7 $NoShortcutMode
  !insertmacro SayAllApplyDesktopChoice
  FileOpen $0 "$EXEDIR\receipt.txt" w
  FileWrite $0 "$SayAllDesktopChoice|$R7|$NoShortcutMode|$UpdateMode|$DesktopRequests|$MenuRequests"
  FileClose $0
SectionEnd
'@
$exe=Join-Path $directory 'fixture.exe'
$nsi=$nsi.Replace('@OUTPUT@',$exe).Replace('@READ@',$read).Replace('@APPLY@',$apply)
$file=Join-Path $directory 'fixture.nsi'
Set-Content -LiteralPath $file -Value $nsi -Encoding utf8
& $MakensisPath /V2 $file *> (Join-Path $directory 'compile.log')
if($LASTEXITCODE -ne 0){throw 'Shortcut choice fixture compilation failed'}
$cases=@(
  @{arguments='/S';expected='|0|0|1|0|0';exit=0},
  @{arguments='/S /SAYALL_DESKTOP=0';expected='0|1|0|1|0|1';exit=0},
  @{arguments='/S /SAYALL_DESKTOP=1';expected='1|0|0|1|1|0';exit=0},
  @{arguments='/S /SAYALL_DESKTOP=2';expected='';exit=87}
)
foreach($case in $cases){
  $process=Start-Process -FilePath $exe -ArgumentList $case.arguments -WindowStyle Hidden -PassThru
  if(!$process.WaitForExit(15000)){throw 'Shortcut choice fixture timed out'}
  if($process.ExitCode -ne $case.exit){throw "Unexpected shortcut fixture exit: $($process.ExitCode)"}
  if($case.exit -eq 0 -and (Get-Content -LiteralPath (Join-Path $directory 'receipt.txt') -Raw) -cne $case.expected){throw 'Shortcut state or register preservation mismatch'}
}
[ordered]@{check='setup_shortcut_choice';result='passed';cases=$cases.Count;real_shortcuts_modified=$false} | ConvertTo-Json
