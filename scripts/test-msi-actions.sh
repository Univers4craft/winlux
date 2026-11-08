#!/bin/sh
# Actions personnalisées EXE de msiexec (#833) : un paquet fabriqué ici par
# wixl et msibuild, avec deux installateurs NSIS (makensis) comme
# programmes à lancer. Vérifie :
#   - type 2 (exe dans la table Binary, différé) : prérequis installé ;
#   - type 18 (fichier installé, après InstallFinalize) : outil installé ;
#   - type 34 à la désinstallation (REMOVE="ALL") : désinstalleur de l'outil
#     lancé depuis le paquet conservé ;
#   - WixQuietExec64 (DLL WiX reconnue) : commande de CustomActionData lancée ;
#   - vraies DLL (mingw-w64) : type 1 immédiat en 32 bits qui lit INSTALLDIR
#     et pose une propriété, type 1 différé en 64 bits qui écrit le fichier
#     nommé par CustomActionData, une action en échec sans arrêter
#     l'installation, type 17 (DLL installée) à REMOVE="ALL" qui l'efface ;
#   - jamais lancés : un programme Linux (/bin/sh), cmd.exe ;
#   - service (ServiceInstall) inscrit dans le registre puis retiré ;
#   - paquet conservé retiré avec l'application.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
for tool in makensis wixl msibuild msiinfo x86_64-w64-mingw32-gcc i686-w64-mingw32-gcc; do
    command -v "$tool" >/dev/null || { echo "$tool absent (paquets nsis, wixl, msitools, gcc-mingw-w64) : test sauté"; exit 0; }
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
echo outil >outil.txt
for n in Prerequis Outil Silencieux; do
    id=$(printf '%s' "$n" | tr 'A-Z' 'a-z')
    cat >"$id.nsi" <<EOF
Unicode true
SetCompressor /SOLID lzma
Name "$n Demo"
OutFile "$id.exe"
InstallDir "\$PROGRAMFILES64\\$n Demo"
Section
  SetOutPath \$INSTDIR
  File outil.txt
  WriteRegStr HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${n}Demo" "DisplayName" "$n Demo"
  WriteUninstaller "\$INSTDIR\\uninst.exe"
SectionEnd
Section "Uninstall"
SectionEnd
EOF
    makensis -V1 "$id.nsi" >/dev/null
done

# Actions DLL écrites contre l'API publique de msi.dll (msiquery.h).
cat >ca.c <<'EOF'
#include <windows.h>
#include <msiquery.h>
UINT __stdcall Prepare(MSIHANDLE h) {
    WCHAR dir[MAX_PATH], data[2 * MAX_PATH]; DWORD n = MAX_PATH;
    if (MsiGetPropertyW(h, L"INSTALLDIR", dir, &n) != ERROR_SUCCESS || n == 0) return ERROR_INSTALL_FAILURE;
    lstrcpyW(data, dir); lstrcatW(data, L"ca-dll.txt");
    MsiSetPropertyW(h, L"EcritDll", data);
    MSIHANDLE r = MsiCreateRecord(2);
    MsiRecordSetStringW(r, 0, L"Pr\x00e9paration dans [1] ([2])");
    MsiRecordSetStringW(r, 1, dir);
    MsiRecordSetInteger(r, 2, 42);
    MsiProcessMessage(h, INSTALLMESSAGE_INFO, r);
    MsiCloseHandle(r);
    return ERROR_SUCCESS;
}
UINT __stdcall Ecrit(MSIHANDLE h) {
    WCHAR path[2 * MAX_PATH]; DWORD n = 2 * MAX_PATH, w;
    if (!MsiGetMode(h, MSIRUNMODE_SCHEDULED)) return ERROR_INSTALL_FAILURE;
    if (MsiGetPropertyW(h, L"CustomActionData", path, &n) != ERROR_SUCCESS || n == 0) return ERROR_INSTALL_FAILURE;
    HANDLE f = CreateFileW(path, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, 0, NULL);
    if (f == INVALID_HANDLE_VALUE) return ERROR_INSTALL_FAILURE;
    WriteFile(f, "dll\n", 4, &w, NULL);
    CloseHandle(f);
    return ERROR_SUCCESS;
}
UINT __stdcall Efface(MSIHANDLE h) {
    WCHAR path[2 * MAX_PATH]; DWORD n = 2 * MAX_PATH;
    MsiGetPropertyW(h, L"CustomActionData", path, &n);
    return DeleteFileW(path) ? ERROR_SUCCESS : ERROR_INSTALL_FAILURE;
}
UINT __stdcall Echoue(MSIHANDLE h) { (void)h; return ERROR_INSTALL_FAILURE; }
EOF
x86_64-w64-mingw32-gcc -shared -O2 -o ca64.dll ca.c -lmsi
i686-w64-mingw32-gcc -shared -O2 -o ca32.dll ca.c -lmsi -Wl,--kill-at

cat >ca.wxs <<'EOF'
<?xml version="1.0" encoding="utf-8"?>
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Product Id="{7B2C6D1F-2222-4333-8444-955566667777}" Name="Winlux Actions" Language="1036" Codepage="1252"
           Version="1.0.0" Manufacturer="Winlux" UpgradeCode="{1F1E1D1C-BBBB-4CCC-8DDD-EEEEFFFF0000}">
    <Package InstallerVersion="200" Compressed="yes" Description="Actions personnalisées"/>
    <Media Id="1" Cabinet="donnees.cab" EmbedCab="yes"/>
    <Directory Id="TARGETDIR" Name="SourceDir">
      <Directory Id="ProgramFiles64Folder">
        <Directory Id="INSTALLDIR" Name="Winlux Actions">
          <Component Id="Principal" Guid="{44444444-2222-4333-8444-555555555555}" Win64="yes">
            <File Id="outil" Name="outil.exe" Source="outil.exe" KeyPath="yes">
              <Shortcut Id="raccourci" Directory="ProgramMenuFolder" Name="Outil Demo" Advertise="no"/>
            </File>
            <File Id="silencieux" Name="silencieux.exe" Source="silencieux.exe"/>
            <File Id="cadll" Name="ca.dll" Source="ca64.dll"/>
            <ServiceInstall Id="service" Name="WinluxDemoSvc" DisplayName="Service de démonstration" Type="ownProcess"
                            Start="auto" ErrorControl="normal" Arguments="--service" Description="Service du paquet de test"/>
            <ServiceControl Id="controle" Name="WinluxDemoSvc" Start="install" Stop="both" Remove="uninstall"/>
          </Component>
        </Directory>
      </Directory>
      <Directory Id="ProgramMenuFolder"/>
    </Directory>
    <Feature Id="Tout" Level="1"><ComponentRef Id="Principal"/></Feature>
  </Product>
</Wix>
EOF
wixl -a x64 -o ca.msi ca.wxs 2>/dev/null
# wixl n'écrit pas ces actions : tables ajoutées par msibuild.
mkdir -p idt/Binary
cp prerequis.exe idt/Binary/Prerequis.exe
msiinfo export ca.msi InstallExecuteSequence >idt/InstallExecuteSequence.idt
printf '%s\t%s\t%s\n' \
    InstallePrerequis 'NOT Installed' 4010 \
    Linux 'NOT Installed' 4020 \
    LanceOutil 'NOT Installed' 6610 \
    RetireOutil 'REMOVE="ALL"' 3400 \
    Nettoie 'REMOVE="ALL"' 3410 \
    PrepareSilencieux 'NOT Installed' 4030 \
    Silencieux 'NOT Installed' 4040 \
    PrepareDll 'NOT Installed' 4050 \
    DonneesEcrit 'NOT Installed' 4060 \
    EcritDll 'NOT Installed' 4070 \
    EchoueDll 'NOT Installed' 4080 \
    DonneesEfface 'REMOVE="ALL"' 3420 \
    EffaceDll 'REMOVE="ALL"' 3430 >>idt/InstallExecuteSequence.idt
{
    printf 'Action\tType\tSource\tTarget\ns72\ti2\tS72\tS255\nCustomAction\tAction\n'
    printf 'InstallePrerequis\t3074\tPrerequis\t/S\n'
    printf 'Linux\t98\tINSTALLDIR\t/bin/sh -c "touch %s/linux-lance"\n' "$work"
    printf 'LanceOutil\t18\toutil\t/S\n'
    printf 'RetireOutil\t34\tINSTALLDIR\t"[ProgramFiles64Folder]Outil Demo\\uninst.exe" /S\n'
    printf 'Nettoie\t98\tINSTALLDIR\tcmd.exe /c del x\n'
    # Motif WiX : propriété CustomActionData posée par une action 51, puis
    # WixQuietExec64 différée dans la DLL WixCA (jamais chargée).
    printf 'PrepareSilencieux\t51\tSilencieux\t"[#silencieux]" /S\n'
    printf 'Silencieux\t3073\tWixCA\tWixQuietExec64\n'
    # Vraies DLL : 1 immédiate (x86), 1 différée (x64, 1025), une en échec,
    # et 17 différée (1041) depuis la DLL installée.
    printf 'PrepareDll\t1\tCaDll32\tPrepare\n'
    printf 'DonneesEcrit\t51\tEcritDll\t[EcritDll]\n'
    printf 'EcritDll\t1025\tCaDll64\tEcrit\n'
    printf 'EchoueDll\t1\tCaDll64\tEchoue\n'
    printf 'DonneesEfface\t51\tEffaceDll\t[INSTALLDIR]ca-dll.txt\n'
    printf 'EffaceDll\t1041\tcadll\tEfface\n'
} >idt/CustomAction.idt
cp prerequis.exe idt/Binary/WixCA.dll
cp ca32.dll idt/Binary/CaDll32.dll
cp ca64.dll idt/Binary/CaDll64.dll
printf 'Name\tData\ns72\tv0\nBinary\tName\nPrerequis\tPrerequis.exe\nWixCA\tWixCA.dll\nCaDll32\tCaDll32.dll\nCaDll64\tCaDll64.dll\n' >idt/Binary.idt
(cd idt && for t in *.idt; do msibuild ../ca.msi -i "$t"; done)

mkdir home
run() { HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" WINLUX_SETUP_RUN_TIMEOUT=120 "$runtime" "$@"; }
fail() { echo "$1" >&2; cat "$work/out.txt" >&2 2>/dev/null || true; exit 1; }
pf="$work/home/Program Files"

run msiexec.exe /i ca.msi /qn >"$work/out.txt" 2>&1 || fail "installation en échec"
test -f "$pf/Winlux Actions/outil.exe" || fail "fichier du paquet absent"
test -f "$pf/Prerequis Demo/outil.txt" || fail "type 2 (table Binary) : prérequis non installé"
test -f "$pf/Outil Demo/outil.txt" || fail "type 18 (fichier installé) : outil non installé"
test -f "$pf/Silencieux Demo/outil.txt" || fail "WixQuietExec64 (CustomActionData) : commande non lancée"
test ! -e "$work/linux-lance" || fail "un programme Linux a été lancé par une action personnalisée"
grep -q 'Action personnalisée Linux non lancée' "$work/out.txt" || fail "action Linux non signalée"
grep -qF 'Préparation dans C:\Program Files\Winlux Actions\ (42)' "$work/out.txt" || fail "DLL x86 : MsiProcessMessage absent du journal"
test -f "$pf/Winlux Actions/ca-dll.txt" || fail "DLL x64 différée : CustomActionData (propriété posée par la DLL x86) non reçue"
grep -q 'Action personnalisée DLL EchoueDll (Echoue) en échec (code 1603)' "$work/out.txt" || fail "DLL en échec non signalée"
ls "$work/home/.local/share/winlux/msi/"*.msi >/dev/null 2>&1 || fail "paquet non conservé pour la désinstallation"
svc='HKLM\SYSTEM\CurrentControlSet\Services\WinluxDemoSvc'
run reg.exe query "$svc" >"$work/svc.txt" 2>&1 || fail "service non inscrit"
grep -q 'outil.exe" --service' "$work/svc.txt" || fail "ImagePath du service : $(cat "$work/svc.txt")"
grep -q 'Start .*(2)' "$work/svc.txt" || fail "service non automatique : $(cat "$work/svc.txt")"

run msiexec.exe /x '{7B2C6D1F-2222-4333-8444-955566667777}' /qn >"$work/out.txt" 2>&1 || fail "désinstallation en échec"
test ! -e "$pf/Winlux Actions/outil.exe" || fail "fichier resté après désinstallation"
test ! -e "$pf/Outil Demo/outil.txt" || fail "type 34 à REMOVE=\"ALL\" : outil non désinstallé"
grep -q 'Action personnalisée DLL EffaceDll (Efface) : terminée' "$work/out.txt" || fail "type 17 à REMOVE=\"ALL\" : DLL installée non appelée"
test -f "$pf/Prerequis Demo/outil.txt" || fail "le prérequis est parti avec l'application"
grep -q 'Action personnalisée Nettoie non lancée : outil système' "$work/out.txt" || fail "cmd.exe non signalé"
if run reg.exe query "$svc" >/dev/null 2>&1; then fail "service resté inscrit après désinstallation"; fi
if ls "$work/home/.local/share/winlux/msi/"*.msi >/dev/null 2>&1; then fail "paquet conservé resté"; fi
echo "msi-actions : DLL x86 et x64 (types 1 et 17, propriétés, CustomActionData), types 2, 18, 34 et WixQuietExec lancés (installation et REMOVE=ALL), service inscrit puis retiré, Linux et cmd refusés, paquet conservé puis retiré"
