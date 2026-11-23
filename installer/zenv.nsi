Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma

!include MUI2.nsh
!include LogicLib.nsh

!ifndef PRODUCTNAME
  !define PRODUCTNAME "Zenv"
!endif
!define MANUFACTURER "Zenv"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCTNAME}"
!define MANUPRODUCTKEY "Software\${MANUFACTURER}\${PRODUCTNAME}"
!define HOME_MARKER "zenv-home.txt"
!define POWERSHELL "$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
!ifndef TAURI_DATA
  !define TAURI_DATA "$LOCALAPPDATA\dev.zenv.desktop"
!endif

Name "${PRODUCTNAME}"
OutFile "${OUTFILE}"
RequestExecutionLevel user
AllowRootDirInstall true
!if "${PRODUCTNAME}" == "Zenv"
  InstallDirRegKey HKCU "${MANUPRODUCTKEY}" ""
!endif

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUCTNAME}"
VIAddVersionKey "CompanyName" "${MANUFACTURER}"
VIAddVersionKey "LegalCopyright" "${MANUFACTURER}"
VIAddVersionKey "FileDescription" "${PRODUCTNAME} Setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_FUNCTION RunZenv
!define MUI_FINISHPAGE_SHOWREADME
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Create a desktop shortcut"
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION CreateDesktopShortcut
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; ---------- install folder

Function .onInit
  ${If} $INSTDIR == ""
    !if "${PRODUCTNAME}" == "Zenv"
      StrCpy $0 $WINDIR 2
      StrCpy $INSTDIR "$0\zenv"
    !else
      StrCpy $INSTDIR "$TEMP\${PRODUCTNAME}"
    !endif
  ${EndIf}
FunctionEnd

!macro ZENV_BAD_IF_UNDER DIR
  StrLen $1 "${DIR}"
  ${If} $1 > 0
    StrCpy $2 $INSTDIR $1
    ${If} $2 == "${DIR}"
      StrCpy $0 1
    ${EndIf}
  ${EndIf}
!macroend

Function ZenvInstDirIsBad
  Push $0
  Push $1
  Push $2
  StrCpy $0 0
  StrLen $1 $INSTDIR
  ${If} $1 <= 3
    StrCpy $0 1
  ${EndIf}
  !insertmacro ZENV_BAD_IF_UNDER "$WINDIR"
  !insertmacro ZENV_BAD_IF_UNDER "$PROGRAMFILES"
  !insertmacro ZENV_BAD_IF_UNDER "$PROGRAMFILES64"
  Pop $2
  Pop $1
  Exch $0
FunctionEnd

Function .onVerifyInstDir
  Call ZenvInstDirIsBad
  Pop $R9
  ${If} $R9 = 1
    Abort
  ${EndIf}
FunctionEnd

; ---------- running Zenv

!macro ZENV_CLOSE_RUNNING UN
Function ${UN}ZenvCloseRunning
  System::Call 'Kernel32::SetEnvironmentVariable(t "ZENV_SETUP_EXE", t "$INSTDIR\zenv.exe")i'
  nsExec::Exec '"${POWERSHELL}" -NoProfile -NonInteractive -Command "if (Get-Process zenv -ErrorAction SilentlyContinue | Where-Object { $$_.Path -eq $$env:ZENV_SETUP_EXE }) { exit 0 } else { exit 1 }"'
  Pop $0
  ${If} $0 == 0
    MessageBox MB_OKCANCEL|MB_ICONEXCLAMATION "${PRODUCTNAME} is running. Setup will close it, which also stops the servers and services it started." /SD IDOK IDOK close
    SetErrorLevel 2
    Quit
    close:
    nsExec::Exec '"${POWERSHELL}" -NoProfile -NonInteractive -Command "$$p = @(Get-Process zenv -ErrorAction SilentlyContinue | Where-Object { $$_.Path -eq $$env:ZENV_SETUP_EXE }); $$p | Stop-Process -Force; $$p | Wait-Process -Timeout 15 -ErrorAction SilentlyContinue"'
    Pop $0
  ${EndIf}
FunctionEnd
!macroend
!insertmacro ZENV_CLOSE_RUNNING ""
!insertmacro ZENV_CLOSE_RUNNING "un."

; ---------- install

Section Install
  Call ZenvInstDirIsBad
  Pop $R9
  ${If} $R9 = 1
    MessageBox MB_ICONSTOP|MB_OK "${PRODUCTNAME} cannot be installed in $INSTDIR. Choose a folder of its own, such as C:\zenv." /SD IDOK
    SetErrorLevel 2
    Quit
  ${EndIf}

  CreateDirectory "$INSTDIR"
  ${IfNot} ${FileExists} "$INSTDIR\${HOME_MARKER}"
    ReadEnvStr $R0 USERDOMAIN
    ReadEnvStr $R1 USERNAME
    nsExec::ExecToLog '"$SYSDIR\icacls.exe" "$INSTDIR" /inheritance:r /grant:r "*S-1-5-18:(OI)(CI)F" "*S-1-5-32-544:(OI)(CI)F" "$R0\$R1:(OI)(CI)F" /C /Q'
    Pop $R2
    ${If} $R2 != 0
      MessageBox MB_ICONSTOP|MB_OK "${PRODUCTNAME} could not limit access to $INSTDIR to your account (icacls: $R2). Choose a folder of your own, such as C:\zenv." /SD IDOK
      SetErrorLevel 2
      Quit
    ${EndIf}
    StrCpy $R3 "1"
  ${Else}
    StrCpy $R3 "0"
  ${EndIf}
  System::Call 'Kernel32::SetEnvironmentVariable(t "ZENV_SETUP_DIR", t "$INSTDIR")i'
  System::Call 'Kernel32::SetEnvironmentVariable(t "ZENV_SETUP_DEEP", t "$R3")i'
  nsExec::Exec `"${POWERSHELL}" -NoProfile -NonInteractive -Command "$$ErrorActionPreference = 'Stop'; $$own = @([Security.Principal.WindowsIdentity]::GetCurrent().User.Value, 'S-1-5-18', 'S-1-5-32-544'); $$items = @(Get-Item -LiteralPath $$env:ZENV_SETUP_DIR -Force) + @(Get-ChildItem -LiteralPath $$env:ZENV_SETUP_DIR -Force -Recurse:($$env:ZENV_SETUP_DEEP -eq '1')); foreach ($$i in $$items) { if ($$own -notcontains (Get-Acl -LiteralPath $$i.FullName).GetOwner([Security.Principal.SecurityIdentifier]).Value) { exit 1 } }; exit 0"`
  Pop $R2
  ${If} $R2 != 0
    MessageBox MB_ICONSTOP|MB_OK "${PRODUCTNAME} cannot be installed in $INSTDIR: it, or something in it, belongs to another account on this computer (or could not be checked). Choose a folder of your own, such as C:\zenv." /SD IDOK
    SetErrorLevel 2
    Quit
  ${EndIf}

  Call ZenvCloseRunning

  SetOutPath "$INSTDIR"
  ClearErrors
  File "/oname=zenv.exe" "${BINARY}"
  ${If} ${Errors}
    MessageBox MB_ICONSTOP|MB_OK "Setup could not replace $INSTDIR\zenv.exe. Close ${PRODUCTNAME} and run Setup again." /SD IDOK
    SetErrorLevel 2
    Quit
  ${EndIf}
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ${If} ${FileExists} "$INSTDIR\WebView2Loader.dll"
    Delete "$INSTDIR\WebView2Loader.dll"
    RMDir /r "${TAURI_DATA}\EBWebView"
  ${EndIf}

  FileOpen $0 "$INSTDIR\${HOME_MARKER}" w
  FileWrite $0 "This folder is the Zenv home: installed versions (bin), databases (data), generated configuration (etc), logs and backups live here.$\r$\n"
  FileWrite $0 "Uninstalling Zenv removes the app only and keeps all of this.$\r$\n"
  FileClose $0

  WriteRegStr HKCU "${MANUPRODUCTKEY}" "" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTKEY}" "MainBinaryName" "zenv.exe"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayName" "${PRODUCTNAME}"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\zenv.exe$\""
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTKEY}" "Publisher" "${MANUFACTURER}"
  WriteRegStr HKCU "${UNINSTKEY}" "InstallLocation" "$\"$INSTDIR$\""
  WriteRegStr HKCU "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${UNINSTKEY}" "EstimatedSize" ${SIZE_KB}

  CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\zenv.exe"
SectionEnd

Function RunZenv
  Exec '"$INSTDIR\zenv.exe"'
FunctionEnd

Function CreateDesktopShortcut
  CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\zenv.exe"
FunctionEnd

; ---------- uninstall

Section Uninstall
  Call un.ZenvCloseRunning

  Delete "$INSTDIR\zenv.exe"
  Delete "$INSTDIR\WebView2Loader.dll"
  Delete "$INSTDIR\${HOME_MARKER}"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  ${If} ${FileExists} "$INSTDIR\*.*"
    DetailPrint "Kept $INSTDIR: it holds your installed versions, databases and settings."
  ${EndIf}

  Delete "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  Delete "$DESKTOP\${PRODUCTNAME}.lnk"
  DeleteRegKey HKCU "${UNINSTKEY}"
  DeleteRegKey HKCU "${MANUPRODUCTKEY}"
  DeleteRegKey /ifempty HKCU "Software\${MANUFACTURER}"
SectionEnd
