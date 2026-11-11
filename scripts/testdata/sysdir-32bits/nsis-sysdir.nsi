; Installateur NSIS x86 de test (#1800) : DLL bidon de Winlux dans $SYSDIR.
; Fabriqué par scripts/test-sysdir-32bits.sh (makensis), ou au banc.
Unicode true
!include "x64.nsh"
Name "Test SYSDIR Winlux"
OutFile "sysdir-test.exe"
InstallDir "$PROGRAMFILES\Test SYSDIR Winlux"
RequestExecutionLevel admin
Section
  ; Redirection WOW64 active : SysWOW64 sous Windows 64 bits.
  SetOutPath $SYSDIR
  File winlux-test-x86.dll
  ; Redirection coupée : le vrai System32.
  ${DisableX64FSRedirection}
  SetOutPath $SYSDIR
  File winlux-test-native.dll
  ; Redirection rétablie : de nouveau SysWOW64.
  ${EnableX64FSRedirection}
  SetOutPath $SYSDIR
  File winlux-test-retour.dll
SectionEnd
