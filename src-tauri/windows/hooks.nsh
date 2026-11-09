; Zenv installer hooks, included by windows/installer.nsi.
;
; The folder Zenv is installed into (C:\zenv by default) is also its home:
; bin\ (installed versions), data\ (databases), etc\, logs\, dev\, backups\
; and zenv.db live next to zenv.exe. The uninstaller removes only the files it
; installed; everything Zenv created or downloaded there is kept.

!define ZENV_HOME_MARKER "zenv-home.txt"

!macro NSIS_HOOK_POSTINSTALL
  ; Tells zenv.exe that this folder is its home.
  FileOpen $0 "$INSTDIR\${ZENV_HOME_MARKER}" w
  FileWrite $0 "This folder is the Zenv home: installed versions (bin), databases (data), generated configuration (etc), logs and backups live here.$\r$\n"
  FileWrite $0 "Uninstalling Zenv removes the app only and keeps all of this.$\r$\n"
  FileClose $0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    Delete "$INSTDIR\${ZENV_HOME_MARKER}"
    ; Removed only when nothing else is left (never recursive).
    RMDir "$INSTDIR"
    ${If} ${FileExists} "$INSTDIR\*.*"
      DetailPrint "Kept $INSTDIR: it holds your installed versions, databases and settings."
    ${EndIf}
  ${EndIf}
!macroend

; Folders that must not become the Zenv home: a drive root, Windows, and
; Program Files (not writable without administrator rights, and its spaces
; trip up some server tools).
!macro ZENV_BAD_IF_UNDER DIR
  StrLen $1 "${DIR}"
  ${If} $1 > 0
    StrCpy $2 $INSTDIR $1
    ${If} $2 == "${DIR}"
      StrCpy $0 1
    ${EndIf}
  ${EndIf}
!macroend

; Leaves 1 on the stack when $INSTDIR is refused, else 0.
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

; Directory page: the Next button stays disabled for a refused folder.
Function .onVerifyInstDir
  Call ZenvInstDirIsBad
  Pop $R9
  ${If} $R9 = 1
    Abort
  ${EndIf}
FunctionEnd

; Silent installs (/S /D=...) skip the directory page, so check again.
!macro NSIS_HOOK_PREINSTALL
  Call ZenvInstDirIsBad
  Pop $R9
  ${If} $R9 = 1
    MessageBox MB_ICONSTOP|MB_OK "Zenv cannot be installed in $INSTDIR. Choose a folder of its own, such as C:\zenv." /SD IDOK
    Quit
  ${EndIf}
  ; A new home inherits "every signed-in account may change" from C:\. Zenv
  ; later starts zenv.exe with administrator rights (hosts file), so limit the
  ; folder to this account, SYSTEM and Administrators before any file lands in
  ; it. An existing home is locked by Zenv itself when it starts.
  ${IfNot} ${FileExists} "$INSTDIR\${ZENV_HOME_MARKER}"
    ReadEnvStr $R0 USERDOMAIN
    ReadEnvStr $R1 USERNAME
    nsExec::ExecToLog '"$SYSDIR\icacls.exe" "$INSTDIR" /inheritance:r /grant:r "*S-1-5-18:(OI)(CI)F" "*S-1-5-32-544:(OI)(CI)F" "$R0\$R1:(OI)(CI)F" /C /Q'
    Pop $R2
    ${If} $R2 != 0
      MessageBox MB_ICONSTOP|MB_OK "Zenv could not limit access to $INSTDIR to your account (icacls: $R2). Choose a folder of your own, such as C:\zenv." /SD IDOK
      Quit
    ${EndIf}
  ${EndIf}
!macroend
