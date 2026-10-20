; The outer executable only extracts and dispatches. The existing Tauri NSIS
; remains the sole owner of installation, upgrade, shortcuts and uninstallation.
Unicode true
ManifestDPIAware true
ManifestDPIAwareness PerMonitorV2
RequestExecutionLevel user
SilentInstall silent
AutoCloseWindow true
SetCompressor /SOLID lzma
SetCompressorDictSize 32

!include LogicLib.nsh
!include WinVer.nsh

!ifndef SAYALL_BOOTSTRAP_LOG_DIR
  !define SAYALL_BOOTSTRAP_LOG_DIR "$LOCALAPPDATA\RemoteCodingLocal\Logs"
!endif
!define WEBVIEW2_CLIENT "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

Name "无线麦 SayAll Windows 版"
Caption "安装无线麦"
BrandingText "SayAll"
OutFile "${SETUP_OUTPUT}"
Icon "${SETUP_ICON}"
VIProductVersion "${SETUP_VERSION}.0"
VIAddVersionKey /LANG=2052 "ProductName" "无线麦 SayAll Windows 版"
VIAddVersionKey /LANG=2052 "FileDescription" "无线麦安装程序"
VIAddVersionKey /LANG=2052 "FileVersion" "${SETUP_VERSION}"
VIAddVersionKey /LANG=2052 "LegalCopyright" "MrPhotato"
VIAddVersionKey /LANG=1033 "ProductName" "SayAll for Windows"
VIAddVersionKey /LANG=1033 "FileDescription" "SayAll Setup"
VIAddVersionKey /LANG=1033 "FileVersion" "${SETUP_VERSION}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "MrPhotato"
LoadLanguageFile "${NSISDIR}\Contrib\Language files\SimpChinese.nlf"
LoadLanguageFile "${NSISDIR}\Contrib\Language files\English.nlf"
LangString UnsupportedWindows 2052 "无线麦需要 Windows 10 1809 或更新版本。"
LangString UnsupportedWindows 1033 "SayAll requires Windows 10 version 1809 or later."
LangString NeedWebView 2052 "安装界面需要 Microsoft Edge WebView2。点击“确定”下载并安装微软官方运行时，然后继续安装无线麦。"
LangString NeedWebView 1033 "This setup needs Microsoft Edge WebView2. Select OK to download and install the official Microsoft runtime, then continue installing SayAll."
LangString WebViewFailed 2052 "WebView2 未能安装完成。请检查网络后重新运行安装程序。"
LangString WebViewFailed 1033 "WebView2 could not be installed. Check your connection and run setup again."
LangString StartFailed 2052 "安装程序未能启动，请重新运行安装包。"
LangString StartFailed 1033 "Setup could not start. Please run the installer again."

Var RawParameters
Var HasParameters
Var ResultCode
Var RuntimePresent
Var LogMessage

Function LogEvent
  ; Coarse diagnostics only: never write command lines, paths or user identity.
  CreateDirectory "${SAYALL_BOOTSTRAP_LOG_DIR}"
  ClearErrors
  FileOpen $9 "${SAYALL_BOOTSTRAP_LOG_DIR}\setup-bootstrap.log" a
  IfErrors done
  FileSeek $9 0 END
  FileWrite $9 "$LogMessage$\r$\n"
  FileClose $9
done:
FunctionEnd

Function ReadOriginalParameters
  ; $CMDLINE/GetParameters removes NSIS's final /D= override. Read the public
  ; Windows command line instead and retain every argument byte-for-byte (UTF-16).
  System::Call 'kernel32::GetCommandLineW() p .r0'
  System::Call 'kernel32::lstrlenW(p r0) i .r1'
  IntOp $2 ${NSIS_MAX_STRLEN} - 1
  IntCmp $1 $2 too_long read_line too_long
too_long:
  StrCpy $LogMessage "event=bootstrap_rejected reason=command_line_too_long"
  Call LogEvent
  SetErrorLevel 1603
  Quit
read_line:
  System::Call 'kernel32::GetCommandLineW() w .r0'
  StrCpy $1 0
  StrCpy $2 $0 1
  StrCmp $2 '$\"' quoted_exe unquoted_exe
quoted_exe:
  IntOp $1 $1 + 1
  StrCpy $2 $0 1 $1
  StrCmp $2 "" exe_done
  StrCmp $2 '$\"' quoted_done quoted_exe
quoted_done:
  IntOp $1 $1 + 1
  Goto exe_done
unquoted_exe:
  StrCpy $2 $0 1 $1
  StrCmp $2 "" exe_done
  StrCmp $2 " " exe_done
  StrCmp $2 "$\t" exe_done
  IntOp $1 $1 + 1
  Goto unquoted_exe
exe_done:
  StrCpy $RawParameters $0 "" $1
  StrCpy $HasParameters 0
  StrCpy $1 0
scan_whitespace:
  StrCpy $2 $RawParameters 1 $1
  StrCmp $2 "" parameters_done
  StrCmp $2 " " next_character
  StrCmp $2 "$\t" next_character
  StrCpy $HasParameters 1
  Goto parameters_done
next_character:
  IntOp $1 $1 + 1
  Goto scan_whitespace
parameters_done:
FunctionEnd

Function DetectWebView
  ; Microsoft's documented 32-bit EdgeUpdate registry locations. Do not confuse
  ; an installed Edge browser, an empty version or 0.0.0.0 with a runtime.
  StrCpy $RuntimePresent 0
  SetRegView 32
  ReadRegStr $0 HKLM "${WEBVIEW2_CLIENT}" "pv"
  StrCmp $0 "" check_user
  StrCmp $0 "0.0.0.0" check_user runtime_found
check_user:
  ReadRegStr $0 HKCU "${WEBVIEW2_CLIENT}" "pv"
  StrCmp $0 "" runtime_done
  StrCmp $0 "0.0.0.0" runtime_done
runtime_found:
  StrCpy $RuntimePresent 1
runtime_done:
FunctionEnd

Function .onInit
  ; Simplified/Traditional Chinese Windows use Chinese; other locales use English.
  System::Call 'kernel32::GetUserDefaultUILanguage() i .r0'
  IntOp $0 $0 & 0x3ff
  StrCpy $LANGUAGE 1033
  ${If} $0 == 4
    StrCpy $LANGUAGE 2052
  ${EndIf}
  ${IfNot} ${AtLeastBuild} 17763
    MessageBox MB_OK|MB_ICONSTOP "$(UnsupportedWindows)"
    SetErrorLevel 1633
    Quit
  ${EndIf}
  Call ReadOriginalParameters
FunctionEnd

Section
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File /oname=sayall-payload.exe "${SETUP_PAYLOAD}"
  ${If} $HasParameters == 1
    ; ANY explicit CLI parameter keeps the existing installer contract. This
    ; avoids substring flag detection (/P inside a path), preserves unknown old
    ; options, and leaves /D exactly last. Only the inner NSIS owns /R restart.
    StrCpy $LogMessage "event=bootstrap_dispatch route=legacy"
    Call LogEvent
    ClearErrors
    ExecWait '$\"$PLUGINSDIR\sayall-payload.exe$\"$RawParameters' $ResultCode
    IfErrors start_failed complete
  ${EndIf}

  Call DetectWebView
  ${If} $RuntimePresent == 0
    StrCpy $LogMessage "event=bootstrap_prerequisite state=missing"
    Call LogEvent
    MessageBox MB_OKCANCEL|MB_ICONINFORMATION "$(NeedWebView)" IDCANCEL cancelled
    ; This is the official bootstrapper, Authenticode-verified during packaging.
    ; It performs the Microsoft runtime download; no network on the present path.
    File /oname=MicrosoftEdgeWebview2Setup.exe "${SETUP_WEBVIEW_BOOTSTRAPPER}"
    ClearErrors
    ExecWait '$\"$PLUGINSDIR\MicrosoftEdgeWebview2Setup.exe$\" /silent /install' $ResultCode
    IfErrors prerequisite_failed
    StrCpy $LogMessage "event=bootstrap_prerequisite exit_code=$ResultCode"
    Call LogEvent
    Call DetectWebView
    ${If} $RuntimePresent == 0
      Goto prerequisite_failed
    ${EndIf}
  ${EndIf}

  File /oname=sayall-setup.exe "${SETUP_HOST}"
  StrCpy $LogMessage "event=bootstrap_dispatch route=gui"
  Call LogEvent
  ClearErrors
  ExecWait '$\"$PLUGINSDIR\sayall-setup.exe$\"' $ResultCode
  IfErrors start_failed complete

cancelled:
  StrCpy $ResultCode 1602
  Goto complete
prerequisite_failed:
  MessageBox MB_OK|MB_ICONSTOP "$(WebViewFailed)"
  StrCpy $ResultCode 1603
  Goto complete
start_failed:
  ; Silent/passive callers receive the failure without a blocking modal dialog.
  ${If} $HasParameters == 0
    MessageBox MB_OK|MB_ICONSTOP "$(StartFailed)"
  ${EndIf}
  StrCpy $ResultCode 1603
complete:
  StrCpy $LogMessage "event=bootstrap_complete exit_code=$ResultCode"
  Call LogEvent
  SetErrorLevel $ResultCode
  Quit
SectionEnd
