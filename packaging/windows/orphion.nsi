; Orphion setup program, built on Linux by packaging/build-windows.sh.
; Source paths use forward slashes (makensis runs on Linux); paths on the
; target system use backslashes.
;
;   makensis -DVERSION=0.3.0 -DROOT=<repository> -DSRC=<folder with the .exe files> \
;            -DOUTFILE=<setup.exe> packaging/windows/orphion.nsi
;
; The setup starts without administrator rights and asks who to install for:
;
; * Just for me: %LOCALAPPDATA%\Programs\Orphion, the user's Start menu,
;   uninstall entry in HKCU. Never asks for administrator rights.
; * Everyone on this PC: Program Files, the shared Start menu, uninstall
;   entry in HKLM. The setup restarts itself through a UAC prompt for this.
;
; Command line: /S silent, /allusers install for everyone (the caller must
; already be elevated when silent), /D=<folder> install folder (last option).

Unicode true
Target amd64-unicode
RequestExecutionLevel user
SetCompressor /SOLID lzma
ManifestDPIAware true

!ifndef VERSION
  !error "pass -DVERSION=x.y.z"
!endif
!ifndef SRC
  !error "pass -DSRC=<folder with orphion.exe, orphionctl.exe and README.txt>"
!endif
!ifndef OUTFILE
  !define OUTFILE "orphion-${VERSION}-windows-x64-setup.exe"
!endif

!define APP "Orphion"
!define PUBLISHER "Orphion contributors"
!define URL "https://github.com/hm-aktaruzzaman-mukdho/Orphion"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orphion"
; Repository root; makensis runs from this script's folder by default.
!ifndef ROOT
  !define ROOT "../.."
!endif

!include MUI2.nsh
!include nsDialogs.nsh
!include LogicLib.nsh
!include FileFunc.nsh
!include WinMessages.nsh

Name "${APP}"
OutFile "${OUTFILE}"
BrandingText "${APP} ${VERSION}"
ShowInstDetails show
ShowUninstDetails show

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${APP} setup"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "Apache License 2.0"

!define MUI_ICON "${ROOT}/assets/orphion.ico"
!define MUI_UNICON "${ROOT}/assets/orphion.ico"
!define MUI_ABORTWARNING
!define MUI_COMPONENTSPAGE_NODESC

; 1 = everyone on this PC, 0 = just for me.
Var AllUsers
Var ModeMe
Var ModeAll

; ---------------------------------------------------------------------------
; Pages
; ---------------------------------------------------------------------------

!define MUI_WELCOMEPAGE_TEXT "This installs ${APP} ${VERSION}, which controls QCY earbuds: noise control, equalizer, touch controls and more.$\r$\n$\r$\nFirmware updates are intentionally not supported.$\r$\n$\r$\nClick Next to continue."
!insertmacro MUI_PAGE_WELCOME
Page custom ModePage ModePageLeave
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "Start ${APP}"
!define MUI_FINISHPAGE_RUN_FUNCTION RunUnelevated
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; ---------------------------------------------------------------------------
; Shared helpers
; ---------------------------------------------------------------------------

; Leaves "1" on the stack when this process runs with administrator rights.
!macro IS_ADMIN
  UserInfo::GetAccountType
  Pop $0
  ${If} $0 == "Admin"
    Push 1
  ${Else}
    Push 0
  ${EndIf}
!macroend

; Stops running copies, which would keep the files locked: Orphion (window
; and tray), the separate tray of earlier versions, and the programs from
; before the rename to Orphion.
!macro STOP_RUNNING
  nsExec::Exec 'taskkill /F /IM orphion.exe'
  Pop $0
  nsExec::Exec 'taskkill /F /IM orphion-tray.exe'
  Pop $0
  nsExec::Exec 'taskkill /F /IM qcy-gui.exe'
  Pop $0
  nsExec::Exec 'taskkill /F /IM qcy-tray.exe'
  Pop $0
  Sleep 500
!macroend

; Adds or removes $INSTDIR in the Path of the install scope.
!macro EDIT_PATH ACTION
  InitPluginsDir
  File "/oname=$PLUGINSDIR\path.ps1" "${ROOT}/packaging/windows/path.ps1"
  ${If} $AllUsers == 1
    StrCpy $1 "Machine"
  ${Else}
    StrCpy $1 "User"
  ${EndIf}
  nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\path.ps1" -Scope $1 -Action ${ACTION} -Dir "$INSTDIR"'
  Pop $0
  SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend

; ---------------------------------------------------------------------------
; Install
; ---------------------------------------------------------------------------

Function .onInit
  StrCpy $AllUsers 0
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/allusers" $R1
  ${IfNot} ${Errors}
    StrCpy $AllUsers 1
  ${Else}
    ; Suggest the scope of an existing installation.
    ReadRegStr $R1 HKLM "${UNINST_KEY}" "InstallLocation"
    ${If} $R1 != ""
      StrCpy $AllUsers 1
    ${EndIf}
    ${If} ${Silent}
      StrCpy $AllUsers 0
    ${EndIf}
  ${EndIf}

  ${If} $AllUsers == 1
    !insertmacro IS_ADMIN
    Pop $0
    ${If} $0 != 1
    ${AndIf} ${Silent}
      ; A silent install for everyone must be started elevated.
      SetErrorLevel 740
      Quit
    ${EndIf}
  ${EndIf}
  Call ApplyMode
FunctionEnd

; Sets the shell context and, unless /D was given, the install folder.
Function ApplyMode
  ${If} $AllUsers == 1
    SetShellVarContext all
    StrCpy $R9 "$PROGRAMFILES64\${APP}"
  ${Else}
    SetShellVarContext current
    StrCpy $R9 "$LOCALAPPDATA\Programs\${APP}"
  ${EndIf}
  ; $INSTDIR is empty unless /D= was given (no InstallDir on purpose).
  ${If} $INSTDIR == ""
  ${OrIf} $INSTDIR == "$PROGRAMFILES64\${APP}"
  ${OrIf} $INSTDIR == "$LOCALAPPDATA\Programs\${APP}"
    StrCpy $INSTDIR $R9
  ${EndIf}
FunctionEnd

Function ModePage
  ; The elevated restart already knows the answer.
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/allusers" $R1
  ${IfNot} ${Errors}
    Abort
  ${EndIf}

  !insertmacro MUI_HEADER_TEXT "Installation type" "Choose who can use ${APP} on this computer."
  nsDialogs::Create 1018
  Pop $0
  ${NSD_CreateRadioButton} 0 0u 100% 12u "&Just for me (recommended)"
  Pop $ModeMe
  ${NSD_CreateLabel} 12u 13u -12u 20u "Installs into your user folder. No administrator rights are needed."
  Pop $0
  ${NSD_CreateRadioButton} 0 40u 100% 12u "&For everyone on this PC"
  Pop $ModeAll
  ${NSD_CreateLabel} 12u 53u -12u 20u "Installs into Program Files for all users. Windows will ask for administrator permission."
  Pop $0
  ${If} $AllUsers == 1
    ${NSD_Check} $ModeAll
  ${Else}
    ${NSD_Check} $ModeMe
  ${EndIf}
  nsDialogs::Show
FunctionEnd

Function ModePageLeave
  ${NSD_GetState} $ModeAll $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $AllUsers 1
    !insertmacro IS_ADMIN
    Pop $0
    ${If} $0 != 1
      ; Restart this setup with administrator rights; it skips this page.
      ClearErrors
      ExecShell "runas" "$EXEPATH" "/allusers"
      ${If} ${Errors}
        MessageBox MB_ICONEXCLAMATION "Installing for everyone needs administrator permission. Choose $\"Just for me$\" to install without it."
        StrCpy $AllUsers 0
        Abort
      ${EndIf}
      Quit
    ${EndIf}
  ${Else}
    StrCpy $AllUsers 0
  ${EndIf}
  Call ApplyMode
FunctionEnd

Section "${APP} (required)" SecApp
  SectionIn RO
  !insertmacro STOP_RUNNING

  ; Leftovers of earlier versions in this scope: the separate tray program
  ; and its shortcuts, and the programs from before the rename (QCY Earbuds).
  Delete "$INSTDIR\orphion-tray.exe"
  Delete "$SMPROGRAMS\${APP}\${APP} tray.lnk"
  Delete "$SMSTARTUP\${APP} tray.lnk"
  Delete "$SMSTARTUP\QCY Earbuds tray.lnk"
  RMDir /r "$SMPROGRAMS\QCY Earbuds"
  ${If} $AllUsers == 0
    RMDir /r "$LOCALAPPDATA\Programs\QCY Earbuds"
    RMDir /r "$LOCALAPPDATA\qcy-earbuds"
  ${EndIf}

  SetOutPath "$INSTDIR"
  File "${SRC}/orphion.exe"
  File "${SRC}/orphionctl.exe"
  File "${SRC}/README.txt"
  File "/oname=LICENSE.txt" "${ROOT}/LICENSE"

  CreateDirectory "$SMPROGRAMS\${APP}"
  CreateShortcut "$SMPROGRAMS\${APP}\${APP}.lnk" "$INSTDIR\orphion.exe" "" "$INSTDIR\orphion.exe" 0 "" "" "Control your QCY earbuds"

  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr SHCTX "${UNINST_KEY}" "DisplayName" "${APP}"
  WriteRegStr SHCTX "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr SHCTX "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr SHCTX "${UNINST_KEY}" "URLInfoAbout" "${URL}"
  WriteRegStr SHCTX "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\orphion.exe"
  WriteRegStr SHCTX "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr SHCTX "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr SHCTX "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD SHCTX "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD SHCTX "${UNINST_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD SHCTX "${UNINST_KEY}" "EstimatedSize" $0
SectionEnd

Section "Start the tray icon when signing in" SecAutostart
  ; For everyone: the shared Startup folder.
  CreateShortcut "$SMSTARTUP\${APP}.lnk" "$INSTDIR\orphion.exe" "--tray" "$INSTDIR\orphion.exe" 0 "" "" "Tray icon for your QCY earbuds"
  ${IfNot} ${Silent}
    ; Start it now, as the signed-in user (not elevated). If a tray is
    ; already running, the new one exits on its own.
    Exec '"$WINDIR\explorer.exe" "$SMSTARTUP\${APP}.lnk"'
  ${EndIf}
SectionEnd

Section /o "Desktop shortcut" SecDesktop
  CreateShortcut "$DESKTOP\${APP}.lnk" "$INSTDIR\orphion.exe" "" "$INSTDIR\orphion.exe" 0 "" "" "Control your QCY earbuds"
SectionEnd

Section /o "Add orphionctl (command line) to the Path" SecPath
  !insertmacro EDIT_PATH Add
  WriteRegDWORD SHCTX "${UNINST_KEY}" "AddedToPath" 1
SectionEnd

; Starts Orphion as the signed-in user, even from an elevated setup.
Function RunUnelevated
  Exec '"$WINDIR\explorer.exe" "$INSTDIR\orphion.exe"'
FunctionEnd

; ---------------------------------------------------------------------------
; Uninstall
; ---------------------------------------------------------------------------

Function un.onInit
  ; An installation for everyone is registered under HKLM.
  StrCpy $AllUsers 0
  ReadRegStr $0 HKLM "${UNINST_KEY}" "InstallLocation"
  ${If} $0 == $INSTDIR
    StrCpy $AllUsers 1
    !insertmacro IS_ADMIN
    Pop $0
    ${If} $0 != 1
      ${If} ${Silent}
        ExecShell "runas" "$INSTDIR\uninstall.exe" "/S"
      ${Else}
        ExecShell "runas" "$INSTDIR\uninstall.exe"
      ${EndIf}
      Quit
    ${EndIf}
    SetShellVarContext all
  ${Else}
    SetShellVarContext current
  ${EndIf}
FunctionEnd

Section "Uninstall"
  !insertmacro STOP_RUNNING

  ReadRegDWORD $0 SHCTX "${UNINST_KEY}" "AddedToPath"
  ${If} $0 == 1
    !insertmacro EDIT_PATH Remove
  ${EndIf}

  Delete "$SMSTARTUP\${APP}.lnk"
  Delete "$DESKTOP\${APP}.lnk"
  RMDir /r "$SMPROGRAMS\${APP}"

  Delete "$INSTDIR\orphion.exe"
  Delete "$INSTDIR\orphionctl.exe"
  Delete "$INSTDIR\README.txt"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey SHCTX "${UNINST_KEY}"

  ; Settings and the endpoint cache belong to the user running this.
  SetShellVarContext current
  MessageBox MB_YESNO|MB_ICONQUESTION "Also remove your ${APP} settings?" /SD IDNO IDNO keep
    RMDir /r "$APPDATA\orphion"
    RMDir /r "$LOCALAPPDATA\orphion"
  keep:
SectionEnd
