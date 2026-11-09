#!/bin/sh
# Actions personnalisées script des paquets MSI (#973), sur un vrai paquet
# construit avec wixl et msibuild :
#   - JScript en ligne (type 37) qui pose une propriété ;
#   - JScript rangé dans une propriété (type 53) ;
#   - VBScript de la table Binary (type 6, fonction Main) : vue SQL sur la
#     table File, Dictionary, On Error Resume Next, Select Case,
#     Session.Message et fichier écrit par Scripting.FileSystemObject ;
#   - JScript différé (type 37 + 0x400) avec son CustomActionData ;
#   - JScript qui rend 3 (échec 1603) et VBScript en erreur (1720) : signalés
#     sans arrêter l'installation ;
#   - VBScript en ligne (type 38) qui écrit le registre (WScript.Shell) ;
#   - VBScript d'un fichier installé (type 22) à la désinstallation.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
# WINLUX_MSI_OUT=<dossier> : construit seulement le paquet de la fiche de
# banc 833 dans ce dossier, sans le runtime (scripts/construire-paquets-833.sh).
out=${WINLUX_MSI_OUT:-}
if [ -n "$out" ]; then mkdir -p "$out"; out=$(CDPATH= cd -- "$out" && pwd); fi
test -n "$out" || test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
for tool in wixl msibuild msiinfo; do
    command -v "$tool" >/dev/null || { [ -n "$out" ] && { echo "$tool absent (paquets wixl, msitools) : paquet non construit" >&2; exit 1; }; echo "$tool absent (paquets wixl, msitools) : test sauté"; exit 0; }
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
echo "Lisez-moi" >lisezmoi.txt
cat >retire.vbs <<'EOF'
' Désinstallation : retire le fichier écrit par Main et la clé de registre.
Function Retire()
  Dim fso, f
  Set fso = CreateObject("Scripting.FileSystemObject")
  For Each f In fso.GetFolder(Session.Property("INSTALLDIR")).Files
    If LCase(Left(f.Name, 4)) = "vbs-" Then f.Delete
  Next
  CreateObject("WScript.Shell").RegDelete "HKCU\Software\WinluxScripts\"
  Retire = 1
End Function
EOF
cat >Verifie.vbs <<'EOF'
Option Explicit
Const msiMessageTypeInfo = &H04000000

Function Main()
  Dim fso, vue, ligne, n, rec, langue, chemin, d
  Set d = CreateObject("Scripting.Dictionary")
  Set vue = Session.Database.OpenView("SELECT `FileName` FROM `File` ORDER BY `File`")
  vue.Execute
  n = 0
  Do
    Set ligne = vue.Fetch
    If ligne Is Nothing Then Exit Do
    n = n + 1
    d.Add ligne.StringData(1), n
  Loop
  vue.Close
  ' Une erreur attrapée n'arrête pas le script.
  On Error Resume Next
  Err.Raise 5
  If Err.Number <> 5 Then Exit Function
  On Error GoTo 0
  Select Case Session.Property("JSVAL")
    Case "js-1036" : langue = "fr"
    Case Else : langue = "autre"
  End Select
  Set rec = Session.Installer.CreateRecord(1)
  rec.StringData(0) = "Message VBScript : [1]"
  rec.StringData(1) = d.Count & " fichiers, " & langue
  Session.Message msiMessageTypeInfo, rec
  chemin = Session.Property("INSTALLDIR") & "vbs-" & Session.Property("JSVAL") & "-" & n & "-" & _
           Session.Property("PROPJS") & ".txt"
  Set fso = CreateObject("Scripting.FileSystemObject")
  With fso.CreateTextFile(chemin, True)
    .WriteLine "ok"
    .Close
  End With
  Session.Property("VBSFAIT") = "1"
  Main = 1
End Function
EOF
cat >Echoue.js <<'EOF'
// Rend msiDoActionStatusFailure (3) : action en échec, installation poursuivie.
function Echec() {
    Session.Property("ECHEC") = "oui";
    return 3;
}
EOF

cat >scripts.wxs <<'EOF'
<?xml version="1.0" encoding="utf-8"?>
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Product Id="{5A1B2C3D-3333-4444-8555-966677778888}" Name="Winlux Scripts" Language="1036" Codepage="1252"
           Version="1.0.0" Manufacturer="Winlux" UpgradeCode="{2F2E2D2C-CCCC-4DDD-8EEE-FFFF00001111}">
    <Package InstallerVersion="200" Compressed="yes" Description="Actions script"/>
    <Media Id="1" Cabinet="donnees.cab" EmbedCab="yes"/>
    <Directory Id="TARGETDIR" Name="SourceDir">
      <Directory Id="ProgramFiles64Folder">
        <Directory Id="INSTALLDIR" Name="Winlux Scripts">
          <Component Id="Principal" Guid="{66666666-3333-4444-8555-777777777777}" Win64="yes">
            <File Id="lisezmoi" Name="lisezmoi.txt" Source="lisezmoi.txt" KeyPath="yes"/>
            <File Id="retire" Name="retire.vbs" Source="retire.vbs"/>
          </Component>
        </Directory>
      </Directory>
    </Directory>
    <Feature Id="Tout" Level="1"><ComponentRef Id="Principal"/></Feature>
  </Product>
</Wix>
EOF
wixl -a x64 -o scripts.msi scripts.wxs 2>/dev/null
# wixl n'écrit pas d'actions script : tables ajoutées par msibuild.
mkdir -p idt/Binary
cp Verifie.vbs idt/Binary/Verifie.vbs
cp Echoue.js idt/Binary/Echoue.js
msiinfo export scripts.msi InstallExecuteSequence >idt/InstallExecuteSequence.idt
msiinfo export scripts.msi Property >idt/Property.idt
printf '%s\t%s\n' SCRIPTJS 'function Fonction() { Session.Property("PROPJS") = "p" + (1 + 1); return 1; }' >>idt/Property.idt
printf '%s\t%s\t%s\n' \
    JsPrepare 'NOT Installed' 4010 \
    JsPropriete 'NOT Installed' 4015 \
    VbsFichiers 'NOT Installed' 4020 \
    DonneesJs 'NOT Installed' 4030 \
    JsDiffere 'NOT Installed' 4040 \
    JsEchoue 'NOT Installed' 4050 \
    VbsErreur 'NOT Installed' 4060 \
    VbsRegistre 'NOT Installed' 4070 \
    VbsRetire 'REMOVE="ALL"' 3410 >>idt/InstallExecuteSequence.idt
{
    printf 'Action\tType\tSource\tTarget\ns72\ti2\tS72\tS255\nCustomAction\tAction\n'
    printf 'JsPrepare\t37\t\tSession.Property("JSVAL") = "js-" + Session.Property("ProductLanguage");\n'
    printf 'JsPropriete\t53\tSCRIPTJS\tFonction\n'
    printf 'VbsFichiers\t6\tVerifie\tMain\n'
    printf 'DonneesJs\t51\tJsDiffere\t[INSTALLDIR]differe.txt\n'
    printf '%s\t%s\t\t%s\n' JsDiffere 1061 'var fso = new ActiveXObject("Scripting.FileSystemObject"); if (!Session.Mode(16)) throw new Error("pas différée"); var f = fso.CreateTextFile(Session.Property("CustomActionData"), true); f.WriteLine("différée"); f.Close();'
    printf 'JsEchoue\t5\tEchoue\tEchec\n'
    printf 'VbsErreur\t38\t\tx = 1 / 0\n'
    printf '%s\t%s\t\t%s\n' VbsRegistre 38 'CreateObject("WScript.Shell").RegWrite "HKCU\Software\WinluxScripts\Fait", Session.Property("VBSFAIT")'
    printf 'VbsRetire\t22\tretire\tRetire\n'
} >idt/CustomAction.idt
printf 'Name\tData\ns72\tv0\nBinary\tName\nVerifie\tVerifie.vbs\nEchoue\tEchoue.js\n' >idt/Binary.idt
(cd idt && for t in *.idt; do msibuild ../scripts.msi -i "$t"; done)
if [ -n "$out" ]; then cp scripts.msi "$out/actions-script.msi"; echo "$out/actions-script.msi"; exit 0; fi

run() { HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" WINLUX_SETUP_RUN_TIMEOUT=120 "$runtime" "$@"; }
fail() { echo "$1" >&2; cat "$work/out.txt" >&2 2>/dev/null || true; exit 1; }
pf="$work/home/Program Files"
dir="$pf/Winlux Scripts"
run msiexec.exe /i scripts.msi /qn >"$work/out.txt" 2>&1 || fail "installation en échec"
test -f "$dir/lisezmoi.txt" || fail "fichier du paquet absent"
grep -q 'Action personnalisée JScript JsPrepare : terminée' "$work/out.txt" || fail "type 37 (JScript en ligne) non exécuté"
grep -q 'Message VBScript : 2 fichiers, fr' "$work/out.txt" || fail "type 6 : Session.Message (vue SQL, Dictionary, Select Case) absent du journal"
test -f "$dir/vbs-js-1036-2-p2.txt" || fail "types 37, 53 et 6 : propriétés JSVAL/PROPJS ou fichier FileSystemObject : $(ls "$dir")"
test -f "$dir/differe.txt" || fail "JScript différé (CustomActionData, Session.Mode) : $(ls "$dir")"
grep -q 'Action personnalisée JScript JsEchoue (Echec) en échec (code 1603)' "$work/out.txt" || fail "script qui rend 3 non signalé en échec"
grep -q 'Action personnalisée VBScript VbsErreur en échec (code 1720)' "$work/out.txt" || fail "erreur de script non signalée (1720)"
grep -q 'division par zéro' "$work/out.txt" || fail "message de l'erreur VBScript absent du journal"
run reg.exe query 'HKCU\Software\WinluxScripts' >"$work/reg.txt" 2>&1 || fail "type 38 : clé de registre non écrite"
grep -q 'Fait.*1' "$work/reg.txt" || fail "type 38 : valeur VBSFAIT (posée par le script précédent) : $(cat "$work/reg.txt")"
if grep -q 'Actions personnalisées non exécutées\|script, type' "$work/out.txt"; then fail "action script encore ignorée"; fi

run msiexec.exe /x '{5A1B2C3D-3333-4444-8555-966677778888}' /qn >"$work/out.txt" 2>&1 || fail "désinstallation en échec"
grep -q 'Action personnalisée VBScript VbsRetire (Retire) : terminée' "$work/out.txt" || fail "type 22 (VBScript installé) non exécuté à REMOVE=\"ALL\""
test ! -e "$dir/vbs-js-1036-2-p2.txt" || fail "type 22 : fichier non retiré par le script"
if run reg.exe query 'HKCU\Software\WinluxScripts' >/dev/null 2>&1; then fail "type 22 : clé de registre restée (RegDelete)"; fi
echo "msi-scripts : JScript (types 5, 37, 53, différé) et VBScript (types 6, 22, 38) exécutés avec Session, vues SQL, FileSystemObject, Dictionary et WScript.Shell ; échecs 1603 et 1720 signalés sans arrêter l'installation"
