param([Parameter(Mandatory=$true)][string]$MakensisPath)
$ErrorActionPreference='Stop'
if (-not ('SayAllSetupLinkReader' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
[ComImport, Guid("00021401-0000-0000-C000-000000000046")]
class SayAllSetupShellLink {}
[ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("000214F9-0000-0000-C000-000000000046")]
interface SayAllSetupIShellLinkW {
    void GetPath([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder path, int length, IntPtr findData, uint flags);
}
public static class SayAllSetupLinkReader {
    public static string ReadTarget(string path) {
        object link = new SayAllSetupShellLink();
        try {
            ((IPersistFile)link).Load(path, 0);
            var target = new StringBuilder(32768);
            ((SayAllSetupIShellLinkW)link).GetPath(target, target.Capacity, IntPtr.Zero, 4);
            return target.ToString();
        } finally { Marshal.FinalReleaseComObject(link); }
    }
}
'@
}
$repo=Split-Path -Parent $PSScriptRoot
$id=[guid]::NewGuid().ToString('N')
$root=Join-Path $repo "target/setup-directory-$id"
$install=Join-Path $root '中文路径 with spaces\SayAll'
$name="SayAll Setup Fixture $id"
$binary="sayall-setup-fixture-$id"
$sourceDirectory=Join-Path $repo 'target/release/nsis/x64'
New-Item -ItemType Directory -Path $root | Out-Null
$csharp=Join-Path $root 'inert.cs'
Set-Content -LiteralPath $csharp -Value 'class Inert { static void Main() {} }' -Encoding utf8
$app=Join-Path $root "$binary.exe"
& (Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe') /nologo /target:winexe "/out:$app" $csharp *> (Join-Path $root 'csharp.log')
if($LASTEXITCODE -ne 0){throw 'Inert executable build failed'}
$text=Get-Content -LiteralPath (Join-Path $sourceDirectory 'installer.nsi') -Raw
function Set-Define([string]$Key,[string]$Value){
  $pattern='(?m)^!define '+[regex]::Escape($Key)+' "[^\r\n]*"\r?$'
  if([regex]::Matches($script:text,$pattern).Count -ne 1){throw "Expected one generated define: $Key"}
  $replacement='!define '+$Key+' "'+$Value.Replace('$','$$')+'"'
  $script:text=[regex]::Replace($script:text,$pattern,[Text.RegularExpressions.MatchEvaluator]{param($m) $replacement})
}
Set-Define PRODUCTNAME $name
Set-Define MANUFACTURER 'SayAllSetupTests'
Set-Define MAINBINARYNAME $binary
Set-Define MAINBINARYSRCPATH $app
Set-Define BUNDLEID "local.sayall.setup.fixture.$id"
Set-Define STARTMENUFOLDER $name
Set-Define OUTFILE (Join-Path $root 'fixture-setup.exe')
Set-Define DISPLAYLANGUAGESELECTOR 'false'
Set-Define INSTALLWEBVIEW2MODE 'skip'
foreach($include in @('utils.nsh','FileAssociation.nsh')){
  $text=$text.Replace('!include "'+$include+'"','!include "'+(Join-Path $sourceDirectory $include)+'"')
}
$resourcePattern='(?s)  ; Copy resources\r?\n.*?(?=  ; Copy external binaries)'
if([regex]::Matches($text,$resourcePattern).Count -ne 1){throw 'Generated resource boundary changed'}
# Keep the actual generated installation/registry/shortcut code. Only replace
# payload bytes and product identity, so this never registers or starts SayAll.
$text=[regex]::Replace($text,$resourcePattern,'  ; Inert fixture has no additional resource payload.'+"`r`n`r`n")
$nsi=Join-Path $root 'fixture.nsi'
Set-Content -LiteralPath $nsi -Value $text -Encoding utf8
& $MakensisPath /V2 $nsi *> (Join-Path $root 'compile.log')
if($LASTEXITCODE -ne 0){throw "Generated NSIS fixture compilation failed: $root/compile.log"}
$desktop=Join-Path ([Environment]::GetFolderPath('Desktop')) "$name.lnk"
$menuRoot=Join-Path ([Environment]::GetFolderPath('Programs')) $name
$menu=Join-Path $menuRoot "$name.lnk"
$flatMenu=Join-Path ([Environment]::GetFolderPath('Programs')) "$name.lnk"
$registry="HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$name"
$product="HKCU:\Software\SayAllSetupTests\$name"
if((Test-Path -LiteralPath $registry) -or (Test-Path -LiteralPath $product) -or (Test-Path -LiteralPath $desktop)){throw 'Fixture identity collision'}
$result=[ordered]@{check='setup_directory_install';result='failed';custom_directory=$false;desktop_opt_out=$false;start_menu_preserved=$false;desktop_opt_in=$false;existing_upgrade=$false;real_app_started=$false}
try {
  foreach($choice in @(0,1)){
    $update=if($choice -eq 1){' /UPDATE'}else{''}
    $process=Start-Process -FilePath (Join-Path $root 'fixture-setup.exe') -ArgumentList "/S$update /SAYALL_DESKTOP=$choice /D=$install" -WindowStyle Hidden -PassThru
    if(!$process.WaitForExit(30000)){throw 'Fixture installer timed out'}
    if($process.ExitCode -ne 0){throw "Fixture installer failed: $($process.ExitCode)"}
    $registered=(Get-ItemProperty -LiteralPath $registry).InstallLocation.Trim('"')
    if($registered -cne $install -or (Get-FileHash -LiteralPath (Join-Path $install "$binary.exe")).Hash -ne (Get-FileHash -LiteralPath $app).Hash){throw 'Installation location or bytes differ'}
    $result.custom_directory=$true
    if($choice -eq 0){
      if(Test-Path -LiteralPath $desktop){throw 'Opt-out created a desktop shortcut'}
      if(!(Test-Path -LiteralPath $menu) -and !(Test-Path -LiteralPath $flatMenu)){throw 'Desktop opt-out suppressed the start menu'}
      $result.desktop_opt_out=$true;$result.start_menu_preserved=$true
    } else {
      if(!(Test-Path -LiteralPath $desktop)){throw 'Explicit opt-in did not create a desktop shortcut'}
      # WScript.Shell.TargetPath uses the ANSI view on some Windows locales and
      # can report ???? for a valid Unicode link. Read the actual wide target.
      $shortcutTarget=[SayAllSetupLinkReader]::ReadTarget($desktop)
      if($shortcutTarget -ine (Join-Path $install "$binary.exe")){throw 'Shortcut points to wrong directory'}
      $result.desktop_opt_in=$true;$result.existing_upgrade=$true
    }
  }
  $result.result='passed'
} finally {
  $result | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'result.json') -Encoding utf8
  Add-Type -AssemblyName Microsoft.VisualBasic
  foreach($file in @($desktop,$menu,$flatMenu)){
    if(Test-Path -LiteralPath $file){[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($file,'OnlyErrorDialogs','SendToRecycleBin')}
  }
  if((Test-Path -LiteralPath $menuRoot) -and @(Get-ChildItem -LiteralPath $menuRoot -Force).Count -eq 0){[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($menuRoot,'OnlyErrorDialogs','SendToRecycleBin')}
  foreach($key in @($registry,$product)){
    if(!$key.EndsWith('\'+$name,[StringComparison]::Ordinal)){throw 'Fixture registry cleanup target mismatch'}
    if(Test-Path -LiteralPath $key){Remove-Item -LiteralPath $key -Recurse -Force}
  }
  # Keep the isolated installation and build evidence under target for inspection.
}
$result | ConvertTo-Json
