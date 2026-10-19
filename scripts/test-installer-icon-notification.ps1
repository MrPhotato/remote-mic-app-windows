$ErrorActionPreference = 'Stop'
$hookPath = Join-Path $PSScriptRoot '../src-tauri/windows/installer-hooks.nsh'
$source = Get-Content -LiteralPath $hookPath -Raw
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
$notifications = $owned + $refresh
Assert-Contract ($source -match '(?m)^!define SAYALL_SHCNE_UPDATEITEM 0x00002000\s*$') 'targeted item notification'
Assert-Contract ($source -match '(?m)^!define SAYALL_SHCNF_PATHW_FLUSHNOWAIT 0x00003005\s*$') 'Unicode path and asynchronous delivery'
Assert-Contract ($owned -match '(?s)\$\{If\} \$\{FileExists\} "\$\{_shortcut\}".*!insertmacro IsShortcutTarget "\$\{_shortcut\}" "\$INSTDIR\\\$\{MAINBINARYNAME\}\.exe".*Pop \$0.*\$\{If\} \$0 = 1.*SHChangeNotify') 'only existing shortcuts targeting the installed app'
Assert-Contract ($refresh -match '(?s)\$\{If\} \$\{FileExists\} "\$INSTDIR\\\$\{MAINBINARYNAME\}\.exe".*SHChangeNotify') 'installed executable must exist'
Assert-Contract ($refresh.Contains('!insertmacro SayAllNotifyOwnedShortcutIcon "$DESKTOP\${PRODUCTNAME}.lnk"')) 'desktop shortcut scope'
Assert-Contract ($refresh.Contains('!insertmacro SayAllNotifyOwnedShortcutIcon "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"')) 'configured start menu folder scope'
Assert-Contract ($refresh.Contains('!insertmacro SayAllNotifyOwnedShortcutIcon "$SMPROGRAMS\${PRODUCTNAME}.lnk"')) 'legacy start menu shortcut scope'
Assert-Contract (([regex]::Matches($refresh, '!insertmacro SayAllNotifyOwnedShortcutIcon ')).Count -eq 3) 'no other shortcut paths'
Assert-Contract ($notifications -notmatch '(?im)^\s*(CreateShortcut|CreateShortCut|Delete|RMDir|Exec|ExecWait|ExecShell|WriteReg\w*|SetShortcutTarget)\b') 'notification does not create, delete, launch or change properties'
Assert-Contract ($notifications -notmatch 'SHCNE_ASSOCCHANGED|SHCNE_ALLEVENTS|ie4uinit|taskkill|IconCache|SHUpdateImage') 'no system-wide cache invalidation or process restart'
Assert-Contract (([regex]::Matches($notifications, 'System::Call')).Count -eq 2) 'only two scoped Shell API call sites'
foreach ($register in @('0','1','2','3')) {
    Assert-Contract ($refresh.Contains('Push $' + $register) -and $refresh.Contains('Pop $' + $register)) "preserves register $register"
}
Assert-Contract ($refresh -match '(?s)Push \$0.*Push \$1.*Push \$2.*Push \$3.*Pop \$3.*Pop \$2.*Pop \$1.*Pop \$0') 'balanced register order'
Assert-Contract ($postinstall.TrimStart().StartsWith('!insertmacro SayAllRefreshInstalledIcons')) 'refresh after file replacement in postinstall'
[pscustomobject]@{result='passed';checks=$passed;scope='installer icon notification contracts';desktop_modified=$false;cache_modified=$false} | ConvertTo-Json
