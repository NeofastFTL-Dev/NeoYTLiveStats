; YTLiveStats installer.
; Per-user install: no administrator rights, everything lands in the user's own profile and the
; entry appears under the current user's "Apps & features" with a working uninstaller.

Unicode true
SetCompressor /SOLID lzma

; The version is supplied by the build, not stored here:
;   makensis /DAPPVERSION=1.2.3 /DAPPVERSION4=1.2.3.0 installer\YTLiveStats.nsi
; APPVERSION4 must be the four numeric parts NSIS wants for VIProductVersion.
!ifndef APPVERSION
  !define APPVERSION "0.0.0"
!endif
!ifndef APPVERSION4
  !define APPVERSION4 "${APPVERSION}.0"
!endif

!define APPNAME "YTLiveStats"
!define APPEXE "YTLiveStats.exe"
!define PUBLISHER "YTLiveStats"
!define REGKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}"

Name "${APPNAME} ${APPVERSION}"
OutFile "..\dist\YTLiveStats-${APPVERSION}-setup.exe"
Icon "..\app.ico"
UninstallIcon "..\app.ico"
InstallDir "$LOCALAPPDATA\Programs\${APPNAME}"
InstallDirRegKey HKCU "${REGKEY}" "InstallLocation"
RequestExecutionLevel user
ShowInstDetails show
ShowUninstDetails show

VIProductVersion "${APPVERSION4}"
VIAddVersionKey "ProductName" "${APPNAME}"
VIAddVersionKey "FileDescription" "${APPNAME} installatieprogramma"
VIAddVersionKey "FileVersion" "${APPVERSION}"
VIAddVersionKey "ProductVersion" "${APPVERSION}"
VIAddVersionKey "LegalCopyright" ""

Page directory
Page instfiles

UninstPage uninstConfirm
UninstPage instfiles

Section "Installatie" SecMain
  SetOutPath "$INSTDIR"
  File "..\dist\publish\${APPEXE}"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\${APPNAME}"
  CreateShortcut "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk" "$INSTDIR\${APPEXE}"

  WriteRegStr HKCU "${REGKEY}" "DisplayName" "${APPNAME}"
  WriteRegStr HKCU "${REGKEY}" "DisplayVersion" "${APPVERSION}"
  WriteRegStr HKCU "${REGKEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${REGKEY}" "DisplayIcon" "$INSTDIR\${APPEXE}"
  WriteRegStr HKCU "${REGKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${REGKEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${REGKEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${REGKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${REGKEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${REGKEY}" "EstimatedSize" 70000
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk"
  RMDir "$SMPROGRAMS\${APPNAME}"
  Delete "$INSTDIR\${APPEXE}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "${REGKEY}"
SectionEnd
