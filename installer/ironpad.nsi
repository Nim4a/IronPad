; IronPad per-user installer (NSIS)
!define APPNAME "IronPad"
Name "${APPNAME}"
OutFile "..\IronPad-Setup.exe"
InstallDir "$LOCALAPPDATA\Programs\IronPad"
RequestExecutionLevel user
Page Directory
Page InstFiles

Section ""
  SetOutPath "$INSTDIR"
  File "..\target\release\ironpad.exe"
  CreateDirectory "$SMPROGRAMS"
  CreateShortCut "$SMPROGRAMS\${APPNAME}.lnk" "$INSTDIR\ironpad.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\ironpad.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${APPNAME}.lnk"
SectionEnd
