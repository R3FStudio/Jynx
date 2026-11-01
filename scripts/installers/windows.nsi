Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
Name "Jynx"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\Jynx Tools"
RequestExecutionLevel user
Icon "${PAYLOAD}\setup\jynx.ico"
SetCompressor /SOLID lzma
Var TestMode
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${PAYLOAD}\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
Function .onInit
 StrCpy $TestMode "0"
 ${GetParameters} $0
 ClearErrors
 ${GetOptions} $0 "/TESTMODE" $1
 IfErrors +2 0
 StrCpy $TestMode "1"
FunctionEnd
Section "Jynx" SEC_MAIN
 SetOutPath "$INSTDIR"
 File /r "${PAYLOAD}\*"
 WriteUninstaller "$INSTDIR\Uninstall.exe"
 ${If} $TestMode == "1"
  FileOpen $0 "$INSTDIR\setup\.test-install" w
  FileClose $0
 ${Else}
  ExecWait '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\setup\windows-path.ps1" -BinDir "$INSTDIR\bin"' $0
  ${If} $0 != 0
   DetailPrint "PATH update failed. Add $INSTDIR\bin to your user PATH manually."
  ${EndIf}
  SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=5000
  CreateDirectory "$SMPROGRAMS\Jynx Tools"
  SetOutPath "$PROFILE"
  CreateShortcut "$SMPROGRAMS\Jynx Tools\Jynx CLI.lnk" "$INSTDIR\bin\jynx-cli.exe"
  CreateShortcut "$SMPROGRAMS\Jynx Tools\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JynxTools" "DisplayName" "Jynx Tools"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JynxTools" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JynxTools" "Publisher" "R3F Studio"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JynxTools" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JynxTools" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JynxTools" "NoRepair" 1
 ${EndIf}
SectionEnd
Section "Uninstall"
 IfFileExists "$INSTDIR\install-manifest.json" 0 done
 IfFileExists "$INSTDIR\setup\.test-install" skip_registration
 ExecWait '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\setup\windows-path.ps1" -BinDir "$INSTDIR\bin" -Remove'
 DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JynxTools"
 Delete "$SMPROGRAMS\Jynx Tools\Jynx CLI.lnk"
 Delete "$SMPROGRAMS\Jynx Tools\Uninstall.lnk"
 RMDir "$SMPROGRAMS\Jynx Tools"
 skip_registration:
 Delete "$INSTDIR\bin\jynx-cli.exe"
 Delete "$INSTDIR\bin\jynx-harness.exe"
 Delete "$INSTDIR\bin\jynx-llama.cmd"
 RMDir "$INSTDIR\bin"
 RMDir /r "$INSTDIR\runtimes"
 RMDir /r "$INSTDIR\licenses"
 RMDir /r "$INSTDIR\source"
 RMDir /r "$INSTDIR\setup"
 Delete "$INSTDIR\LICENSE"
 Delete "$INSTDIR\COPYRIGHT"
 Delete "$INSTDIR\README.txt"
 Delete "$INSTDIR\install.sh"
 Delete "$INSTDIR\uninstall.sh"
 Delete "$INSTDIR\runtime-manifest.json"
 Delete "$INSTDIR\install-manifest.json"
 Delete "$INSTDIR\Uninstall.exe"
 RMDir "$INSTDIR"
 done:
SectionEnd
