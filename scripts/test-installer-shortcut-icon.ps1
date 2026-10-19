param(
    [Parameter(Mandatory = $true)][string]$MakensisPath,
    [string]$UtilsPath = (Join-Path $PSScriptRoot '../target/release/nsis/x64/utils.nsh')
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$fixture = Join-Path $repo ('target/shortcut-icon-fixture-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
$config = Get-Content -LiteralPath (Join-Path $repo 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$iconName = $config.bundle.resources.'icons/icon.ico'
if ($iconName -notmatch '^sayall-shortcut-[a-f0-9]{12}\.ico$') { throw 'Invalid bundled icon name' }
$target = Join-Path $fixture 'fixture-target.exe'
$otherTarget = Join-Path $fixture 'other-target.exe'
Set-Content -LiteralPath $target -Value 'Non-executable fixture; never launched.'
Set-Content -LiteralPath $otherTarget -Value 'Non-executable fixture; never launched.'
Copy-Item -LiteralPath (Join-Path $repo 'src-tauri/icons/icon.ico') -Destination (Join-Path $fixture $iconName)
Copy-Item -LiteralPath (Join-Path $repo 'src-tauri/icons/icon.ico') -Destination (Join-Path $fixture 'custom.ico')
$shell = New-Object -ComObject WScript.Shell
function Read-Link([string]$Path) {
    $link = $shell.CreateShortcut($Path)
    return [ordered]@{
        target = $link.TargetPath; arguments = $link.Arguments
        working = $link.WorkingDirectory; hotkey = $link.Hotkey
        description = $link.Description; show = $link.WindowStyle; icon = $link.IconLocation
    }
}
$cases = @(
    @{name='default'; target=$target; icon=',0'; change=$true},
    @{name='explicit-default'; target=$target; icon=($target+',0'); change=$true},
    @{name='wrong-target'; target=$otherTarget; icon=',0'; change=$false},
    @{name='custom'; target=$target; icon=((Join-Path $fixture 'custom.ico')+',0'); change=$false},
    @{name='custom-index'; target=$target; icon=($target+',1'); change=$false},
    @{name='already-owned'; target=$target; icon=((Join-Path $fixture $iconName)+',0'); change=$false}
)
foreach ($case in $cases) {
    $case.path = Join-Path $fixture ($case.name+'.lnk')
    $link = $shell.CreateShortcut($case.path)
    $link.TargetPath = $case.target
    $link.Arguments = '--fixture "preserve arguments"'
    $link.WorkingDirectory = $fixture
    $link.Hotkey = 'CTRL+ALT+Q'
    $link.Description = 'Preserve shortcut properties'
    $link.WindowStyle = 3
    $link.IconLocation = $case.icon
    $link.Save()
    $case.before = Read-Link $case.path
    $case.hash = (Get-FileHash -LiteralPath $case.path).Hash
}
$corrupt = Join-Path $fixture 'corrupt.lnk'
Set-Content -LiteralPath $corrupt -Value 'Invalid link fixture'
$corruptHash = (Get-FileHash -LiteralPath $corrupt).Hash
$script = @'
Unicode true
SilentInstall silent
!include MUI2.nsh
!include FileFunc.nsh
!include "Win\COM.nsh"
!include "Win\Propkey.nsh"
!include "@UTILS@"
!include "@HOOKS@"
!define MAINBINARYNAME "fixture-target"
!define PRODUCTNAME "SayAll icon fixture"
Var AppStartMenuFolder
Name "SayAll shortcut icon fixture"
OutFile "@OUTPUT@"
RequestExecutionLevel user
Section
  StrCpy $INSTDIR "@FIXTURE@"
  StrCpy $0 "preserve0"
  StrCpy $1 "preserve1"
  StrCpy $2 "preserve2"
  StrCpy $3 "preserve3"
  StrCpy $4 "preserve4"
  StrCpy $5 "preserve5"
  StrCpy $6 "preserve6"
  Push "stack-sentinel"
  !insertmacro SayAllNotifyOwnedShortcutIcon "$INSTDIR\default.lnk"
  !insertmacro SayAllNotifyOwnedShortcutIcon "$INSTDIR\explicit-default.lnk"
  !insertmacro SayAllNotifyOwnedShortcutIcon "$INSTDIR\wrong-target.lnk"
  !insertmacro SayAllNotifyOwnedShortcutIcon "$INSTDIR\custom.lnk"
  !insertmacro SayAllNotifyOwnedShortcutIcon "$INSTDIR\custom-index.lnk"
  !insertmacro SayAllNotifyOwnedShortcutIcon "$INSTDIR\already-owned.lnk"
  !insertmacro SayAllNotifyOwnedShortcutIcon "$INSTDIR\missing.lnk"
  !insertmacro SayAllNotifyOwnedShortcutIcon "$INSTDIR\corrupt.lnk"
  StrCmp $0 "preserve0" 0 failed
  StrCmp $1 "preserve1" 0 failed
  StrCmp $2 "preserve2" 0 failed
  StrCmp $3 "preserve3" 0 failed
  StrCmp $4 "preserve4" 0 failed
  StrCmp $5 "preserve5" 0 failed
  StrCmp $6 "preserve6" 0 failed
  Pop $0
  StrCmp $0 "stack-sentinel" 0 failed
  SetErrorLevel 0
  Goto done
failed:
  SetErrorLevel 7
done:
SectionEnd
'@
$script = $script.Replace('@UTILS@',(Resolve-Path $UtilsPath).Path).Replace('@HOOKS@',(Join-Path $repo 'src-tauri/windows/installer-hooks.nsh')).Replace('@OUTPUT@',(Join-Path $fixture 'icon-fixture.exe')).Replace('@FIXTURE@',$fixture)
$nsi = Join-Path $fixture 'icon-fixture.nsi'
$script | Set-Content -LiteralPath $nsi -Encoding utf8
& $MakensisPath /V2 $nsi *> (Join-Path $fixture 'compile.log')
if ($LASTEXITCODE -ne 0) { throw "Fixture compilation failed; see $fixture/compile.log" }
for ($run = 1; $run -le 2; $run++) {
    $process = Start-Process -FilePath (Join-Path $fixture 'icon-fixture.exe') -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Fixture failed with exit code $($process.ExitCode)" }
    foreach ($case in $cases) {
        $after = Read-Link $case.path
        foreach ($property in @('target','arguments','working','hotkey','description','show')) {
            if ($after[$property] -cne $case.before[$property]) { throw "$($case.name) changed $property" }
        }
        $expectedIcon = if ($case.change) { (Join-Path $fixture $iconName)+',0' } else { $case.before.icon }
        if ($after.icon -cne $expectedIcon) { throw "$($case.name) unexpected icon: $($after.icon)" }
        $hash = (Get-FileHash -LiteralPath $case.path).Hash
        if ((-not $case.change -or $run -eq 2) -and $hash -ne $case.hash) { throw "$($case.name) was rewritten unexpectedly" }
        $case.hash = $hash
    }
    if ((Get-FileHash -LiteralPath $corrupt).Hash -ne $corruptHash) { throw 'Invalid link was changed' }
    if (Test-Path -LiteralPath (Join-Path $fixture 'missing.lnk')) { throw 'Missing link was created' }
}
# Reuse the same production function with a missing resource. Existing links must
# remain byte-identical; move the fixture resource inside its own test directory.
Move-Item -LiteralPath (Join-Path $fixture $iconName) -Destination (Join-Path $fixture 'resource-held-for-test.ico')
$process = Start-Process -FilePath (Join-Path $fixture 'icon-fixture.exe') -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0) { throw 'Missing-resource fixture failed' }
foreach ($case in $cases) {
    if ((Get-FileHash -LiteralPath $case.path).Hash -ne $case.hash) { throw 'Missing resource changed a shortcut' }
}
$result = [ordered]@{result='passed';cases=8;runs=3;properties_preserved=6;registers_preserved=7;stack_preserved=$true;idempotent=$true;missing_resource_safe=$true;user_desktop_modified=$false;fixture=$fixture}
$result | ConvertTo-Json | Tee-Object -FilePath (Join-Path $fixture 'result.json')
