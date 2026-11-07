#!/bin/sh
# Actions personnalisées EXE de msiexec (#833) : un paquet fabriqué ici par
# wixl et msibuild, avec deux installateurs NSIS (makensis) comme
# programmes à lancer. Vérifie :
#   - type 2 (exe dans la table Binary, différé) : prérequis installé ;
#   - type 18 (fichier installé, après InstallFinalize) : outil installé ;
#   - type 34 à la désinstallation (REMOVE="ALL") : désinstalleur de l'outil
#     lancé depuis le paquet conservé ;
#   - jamais lancés : un programme Linux (/bin/sh), cmd.exe ;
#   - service (ServiceInstall) inscrit dans le registre puis retiré ;
#   - paquet conservé retiré avec l'application.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
for tool in makensis wixl msibuild msiinfo; do
    command -v "$tool" >/dev/null || { echo "$tool absent (paquets nsis, wixl, msitools) : test sauté"; exit 0; }
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
echo outil >outil.txt
for n in Prerequis Outil; do
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
    Nettoie 'REMOVE="ALL"' 3410 >>idt/InstallExecuteSequence.idt
{
    printf 'Action\tType\tSource\tTarget\ns72\ti2\tS72\tS255\nCustomAction\tAction\n'
    printf 'InstallePrerequis\t3074\tPrerequis\t/S\n'
    printf 'Linux\t98\tINSTALLDIR\t/bin/sh -c "touch %s/linux-lance"\n' "$work"
    printf 'LanceOutil\t18\toutil\t/S\n'
    printf 'RetireOutil\t34\tINSTALLDIR\t"[ProgramFiles64Folder]Outil Demo\\uninst.exe" /S\n'
    printf 'Nettoie\t98\tINSTALLDIR\tcmd.exe /c del x\n'
} >idt/CustomAction.idt
printf 'Name\tData\ns72\tv0\nBinary\tName\nPrerequis\tPrerequis.exe\n' >idt/Binary.idt
(cd idt && for t in *.idt; do msibuild ../ca.msi -i "$t"; done)

mkdir home
run() { HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" WINLUX_SETUP_RUN_TIMEOUT=120 "$runtime" "$@"; }
fail() { echo "$1" >&2; cat "$work/out.txt" >&2 2>/dev/null || true; exit 1; }
pf="$work/home/Program Files"

run msiexec.exe /i ca.msi /qn >"$work/out.txt" 2>&1 || fail "installation en échec"
test -f "$pf/Winlux Actions/outil.exe" || fail "fichier du paquet absent"
test -f "$pf/Prerequis Demo/outil.txt" || fail "type 2 (table Binary) : prérequis non installé"
test -f "$pf/Outil Demo/outil.txt" || fail "type 18 (fichier installé) : outil non installé"
test ! -e "$work/linux-lance" || fail "un programme Linux a été lancé par une action personnalisée"
grep -q 'Action personnalisée Linux non lancée' "$work/out.txt" || fail "action Linux non signalée"
ls "$work/home/.local/share/winlux/msi/"*.msi >/dev/null 2>&1 || fail "paquet non conservé pour la désinstallation"
svc='HKLM\SYSTEM\CurrentControlSet\Services\WinluxDemoSvc'
run reg.exe query "$svc" >"$work/svc.txt" 2>&1 || fail "service non inscrit"
grep -q 'outil.exe" --service' "$work/svc.txt" || fail "ImagePath du service : $(cat "$work/svc.txt")"
grep -q 'Start .*(2)' "$work/svc.txt" || fail "service non automatique : $(cat "$work/svc.txt")"

run msiexec.exe /x '{7B2C6D1F-2222-4333-8444-955566667777}' /qn >"$work/out.txt" 2>&1 || fail "désinstallation en échec"
test ! -e "$pf/Winlux Actions/outil.exe" || fail "fichier resté après désinstallation"
test ! -e "$pf/Outil Demo/outil.txt" || fail "type 34 à REMOVE=\"ALL\" : outil non désinstallé"
test -f "$pf/Prerequis Demo/outil.txt" || fail "le prérequis est parti avec l'application"
grep -q 'Action personnalisée Nettoie non lancée : outil système' "$work/out.txt" || fail "cmd.exe non signalé"
if run reg.exe query "$svc" >/dev/null 2>&1; then fail "service resté inscrit après désinstallation"; fi
if ls "$work/home/.local/share/winlux/msi/"*.msi >/dev/null 2>&1; then fail "paquet conservé resté"; fi
echo "msi-actions : types 2, 18 et 34 lancés (installation et REMOVE=ALL), service inscrit puis retiré, Linux et cmd refusés, paquet conservé puis retiré"
