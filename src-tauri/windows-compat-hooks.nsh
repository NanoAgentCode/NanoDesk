; Keep the existing document-chat uninstall cleanup for compatibility packages.
!include "${__FILEDIR__}\document-chat-hooks.nsh"
!include "WinVer.nsh"

; Tauri's PREINSTALL hook runs AFTER its WebView2 section. Declare an earlier
; hidden section here so missing legacy runtimes abort before any download.
Section "-NanoDeskCompatibility"
  ${IfNot} ${AtLeastWin7}
    MessageBox MB_OK|MB_ICONSTOP "NanoDesk requires Windows 7 SP1 or later." /SD IDOK
    SetErrorLevel 1603
    Abort
  ${EndIf}
  ${If} ${IsWin7}
  ${AndIfNot} ${AtLeastServicePack} 1
    MessageBox MB_OK|MB_ICONSTOP "NanoDesk requires Windows 7 Service Pack 1." /SD IDOK
    SetErrorLevel 1603
    Abort
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    Push $R0
    Push $R1
    ; Modern WebView2 bootstrappers no longer start on Windows 7/8.
    ; Require a separately provisioned Microsoft WebView2 109 runtime first.
    SetRegView 32
    ReadRegStr $R0 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
    ${If} $R0 == ""
      ReadRegStr $R0 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
    ${EndIf}
    SetRegView lastused
    StrCpy $R1 $R0 4
    ${If} $R1 != "109."
      MessageBox MB_OK|MB_ICONSTOP "On Windows 7/8/8.1, install Microsoft Edge WebView2 Runtime 109 for your system architecture first. This runtime is no longer security-updated. Then run this installer again." /SD IDOK
      SetErrorLevel 1603
      Abort
    ${EndIf}
    Pop $R1
    Pop $R0
  ${EndIf}
SectionEnd
