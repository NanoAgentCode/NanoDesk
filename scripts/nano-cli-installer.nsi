Unicode true

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "WinMessages.nsh"

!ifndef CLI_EXE
  !error "CLI_EXE is required"
!endif
!ifndef PATH_HELPER
  !error "PATH_HELPER is required"
!endif
!ifndef OUTPUT_FILE
  !error "OUTPUT_FILE is required"
!endif
!ifndef PRODUCT_VERSION
  !define PRODUCT_VERSION "0.1.0"
!endif
!ifndef PRODUCT_NAME
  !error "PRODUCT_NAME is required"
!endif
!ifndef CLI_REGISTRY_PATH
  !error "CLI_REGISTRY_PATH is required"
!endif
!ifndef CLI_UNINSTALL_KEY
  !error "CLI_UNINSTALL_KEY is required"
!endif

Name "${PRODUCT_NAME} CLI"
OutFile "${OUTPUT_FILE}"
InstallDir "$PROFILE\.nano"
InstallDirRegKey HKCU "${CLI_REGISTRY_PATH}" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
ShowInstDetails show
ShowUninstDetails show

!ifdef APP_ICON
  Icon "${APP_ICON}"
  UninstallIcon "${APP_ICON}"
!endif

!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\nano.exe"
!define MUI_FINISHPAGE_RUN_PARAMETERS "--help"
!define MUI_FINISHPAGE_RUN_TEXT "Run nano --help"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "SimpChinese"

Section "${PRODUCT_NAME} CLI" SecCli
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "/oname=nano.exe" "${CLI_EXE}"
  File "/oname=update-user-path.ps1" "${PATH_HELPER}"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\update-user-path.ps1" -Action Add -Path "$INSTDIR"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "$1"
    Abort "Failed to add $INSTDIR to the user PATH (exit code $0)"
  ${EndIf}

  WriteRegStr HKCU "${CLI_REGISTRY_PATH}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${CLI_UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME} CLI"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${CLI_UNINSTALL_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${CLI_UNINSTALL_KEY}" "Publisher" "${PRODUCT_NAME}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${CLI_UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${CLI_UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${CLI_UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${CLI_UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${CLI_UNINSTALL_KEY}" "NoRepair" 1

  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  IfFileExists "$INSTDIR\update-user-path.ps1" 0 path_done
  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\update-user-path.ps1" -Action Remove -Path "$INSTDIR"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "$1"
    Abort "Failed to remove $INSTDIR from the user PATH (exit code $0)"
  ${EndIf}

path_done:
  Delete "$INSTDIR\nano.exe"
  Delete "$INSTDIR\update-user-path.ps1"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "${CLI_REGISTRY_PATH}"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${CLI_UNINSTALL_KEY}"
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
SectionEnd
