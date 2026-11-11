; Installeur Inno Setup en mode 32 bits de test (#1796) : DLL bidon de
; Winlux dans {sys}, qui doit arriver dans C:\Windows\SysWOW64 ; la seconde,
; avec {sysnative}, dans System32. Pas d'ArchitecturesInstallIn64BitMode.
; À compiler avec ISCC (Inno Setup 6, jrsoftware.org) ; à côté du .iss :
; winlux-test-x86.dll et winlux-test-native.dll (deux fichiers texte suffisent).
[Setup]
AppName=Test SYS Winlux
AppVersion=1.0
DefaultDirName={autopf}\Test SYS Winlux
OutputBaseFilename=inno-sys-test
OutputDir=.
PrivilegesRequired=admin
DisableProgramGroupPage=yes

[Files]
Source: "winlux-test-x86.dll"; DestDir: "{sys}"; Flags: ignoreversion uninsneveruninstall
Source: "winlux-test-native.dll"; DestDir: "{sysnative}"; Flags: ignoreversion uninsneveruninstall
