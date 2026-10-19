$ErrorActionPreference = 'Stop'
$hookPath = Join-Path $PSScriptRoot '../src-tauri/windows/installer-hooks.nsh'
$source = Get-Content -LiteralPath $hookPath -Raw
$config = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
function Read-Macro([string]$Name) {
    $match = [regex]::Match($source, '(?ms)^!macro ' + [regex]::Escape($Name) + '(?:\s[^\r\n]*)?\r?\n(.*?)^!macroend\s*$')
    if (-not $match.Success) { throw "Missing installer macro: $Name" }
    return $match.Groups[1].Value
}
function Assert-Contract([bool]$Condition, [string]$Name) {
    if (-not $Condition) { throw "Installer icon notification contract failed: $Name" }
    $script:passed++
}
$passed = 0
$owned = Read-Macro 'SayAllNotifyOwnedShortcutIcon'
$refresh = Read-Macro 'SayAllRefreshInstalledIcons'
$postinstall = Read-Macro 'NSIS_HOOK_POSTINSTALL'
$function = [regex]::Match($source, '(?ms)^Function SayAllRefreshShortcutIcon\r?\n(.*?)^FunctionEnd').Groups[1].Value
$notifications = $owned + $refresh + $function
$iconHash = (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot '../src-tauri/icons/icon.ico')).Hash.ToLowerInvariant().Substring(0,12)
$iconName = 'sayall-shortcut-' + $iconHash + '.ico'
Assert-Contract ($config.bundle.resources.'icons/icon.ico' -eq $iconName) 'bundle uses a content-versioned standalone icon'
Assert-Contract ($source.Contains('!define SAYALL_SHORTCUT_ICON "' + $iconName + '"')) 'installer uses the bundled icon version'
Assert-Contract ($source -match '(?m)^!define SAYALL_SHCNE_UPDATEITEM 0x00002000\s*$') 'targeted item notification'
Assert-Contract ($source -match '(?m)^!define SAYALL_SHCNF_PATHW_FLUSHNOWAIT 0x00003005\s*$') 'Unicode path and asynchronous delivery'
Assert-Contract ($owned -match '(?s)Push "\$INSTDIR\\\$\{MAINBINARYNAME\}\.exe".*Push "\$\{_shortcut\}".*Call SayAllRefreshShortcutIcon') 'fixed expected target passed to icon update'
Assert-Contract ($function -match '(?s)IfFileExists "\$5" 0 sayall_icon_done.*IPersistFile::Load.*IShellLink::GetPath.*StrCmp \$2 "\$6" 0 sayall_icon_wrong_target.*IShellLink::SetIconLocation.*IPersistFile::Save') 'only loaded existing links with matching targets can change'
Assert-Contract ($function -match '(?s)StrCmp \$3 0 0 sayall_icon_custom.*StrCmp \$2 "\$INSTDIR\\\$\{SAYALL_SHORTCUT_ICON\}" sayall_icon_notify.*StrCmp \$2 "" sayall_icon_set.*StrCmp \$2 "\$6" sayall_icon_set sayall_icon_custom') 'only default icons migrate and existing owned icon avoids rewrite'
Assert-Contract ($function -match 'IfFileExists "\$INSTDIR\\\$\{SAYALL_SHORTCUT_ICON\}" 0 sayall_icon_missing') 'missing bundled icon leaves links untouched'
Assert-Contract (([regex]::Matches($function, '\$\{IShellLink::Set\w+\}')).Count -eq 1) 'only icon property is set'
Assert-Contract (([regex]::Matches($function, '\$\{If\} \$4 != 0')).Count -eq 6) 'COM load read and save errors checked'
Assert-Contract ($refresh -match '(?s)\$\{If\} \$\{FileExists\} "\$INSTDIR\\\$\{MAINBINARYNAME\}\.exe".*SHChangeNotify') 'installed executable must exist'
Assert-Contract ($refresh.Contains('!insertmacro SayAllNotifyOwnedShortcutIcon "$DESKTOP\${PRODUCTNAME}.lnk"')) 'desktop shortcut scope'
Assert-Contract ($refresh.Contains('!insertmacro SayAllNotifyOwnedShortcutIcon "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"')) 'configured start menu folder scope'
Assert-Contract ($refresh.Contains('!insertmacro SayAllNotifyOwnedShortcutIcon "$SMPROGRAMS\${PRODUCTNAME}.lnk"')) 'legacy start menu shortcut scope'
Assert-Contract (([regex]::Matches($refresh, '!insertmacro SayAllNotifyOwnedShortcutIcon ')).Count -eq 3) 'no other shortcut paths'
Assert-Contract ($notifications -notmatch '(?im)^\s*(CreateShortcut|CreateShortCut|Delete|RMDir|Exec|ExecWait|ExecShell|WriteReg\w*|SetShortcutTarget)\b') 'notification does not create, delete, launch or retarget links'
Assert-Contract ($notifications -notmatch 'SHCNE_ASSOCCHANGED|SHCNE_ALLEVENTS|ie4uinit|taskkill|IconCache|SHUpdateImage') 'no system-wide cache invalidation or process restart'
Assert-Contract (([regex]::Matches($notifications, 'System::Call')).Count -eq 2) 'only two scoped Shell API call sites'
foreach ($register in @('0','1','2','3')) {
    Assert-Contract ($refresh.Contains('Push $' + $register) -and $refresh.Contains('Pop $' + $register)) "preserves register $register"
}
Assert-Contract ($refresh -match '(?s)Push \$0.*Push \$1.*Push \$2.*Push \$3.*Pop \$3.*Pop \$2.*Pop \$1.*Pop \$0') 'balanced register order'
Assert-Contract ($postinstall.TrimStart().StartsWith('!insertmacro SayAllRefreshInstalledIcons')) 'refresh after file replacement in postinstall'
[pscustomobject]@{result='passed';checks=$passed;scope='installer icon notification contracts';desktop_modified=$false;cache_modified=$false} | ConvertTo-Json
