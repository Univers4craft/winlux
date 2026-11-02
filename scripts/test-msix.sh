#!/bin/sh
# Paquets MSIX (#272) : construit un petit .msix (manifeste, block map,
# hello.exe) et un .msixbundle, puis vérifie sous Winlux
#   - l'installation dans C:\Program Files\WindowsApps\<PackageFullName> ;
#   - le raccourci du menu Démarrer, l'association .wlxdoc et le protocole ;
#   - l'entrée de « Applications installées » et le dossier LocalState ;
#   - que le programme installé se lance ;
#   - le refus d'un paquet dont un fichier a été modifié ;
#   - le choix du paquet x64 dans un bundle ;
#   - la désinstallation complète.
# Sauté (code 0) quand le runtime ou hello.exe manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
hello=${WINLUX_TESTDIR:-$root/target/x86_64-pc-windows-msvc/release}/hello.exe
test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
test -f "$hello" || { echo "hello.exe absent ($hello) : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
home="$work/home"
mkdir -p "$home/.config"
export HOME="$home" XDG_CONFIG_HOME="$home/.config" XDG_DATA_HOME="$home/.local/share"
export WINLUX_HOME="$home" WINLUX_DRIVE_C="$home" WINLUX_GUI=headless

python3 -I - "$hello" "$work" <<'PY'
import base64, hashlib, sys, zipfile
hello, work = sys.argv[1], sys.argv[2]
manifest = """<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10">
  <Identity Name="Winlux.TestMsix" Publisher="CN=Winlux Test" Version="1.0.0.0" ProcessorArchitecture="ARCH" />
  <Properties><DisplayName>Test MSIX Winlux</DisplayName><PublisherDisplayName>Winlux</PublisherDisplayName></Properties>
  <Applications>
    <Application Id="Hello" Executable="bin/hello.exe" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="Bonjour MSIX" Description="test" />
      <Extensions>
        <uap:Extension Category="windows.fileTypeAssociation">
          <uap:FileTypeAssociation Name="wlxdoc"><uap:SupportedFileTypes><uap:FileType>.wlxdoc</uap:FileType></uap:SupportedFileTypes></uap:FileTypeAssociation>
        </uap:Extension>
        <uap:Extension Category="windows.protocol"><uap:Protocol Name="winlux-msix" /></uap:Extension>
      </Extensions>
    </Application>
  </Applications>
</Package>
"""
def package(path, arch, tamper=False):
    files = {
        "AppxManifest.xml": manifest.replace("ARCH", arch).encode(),
        "bin/hello.exe": open(hello, "rb").read(),
        "Assets/Logo grand.txt": b"logo",
    }
    blocks = ['<?xml version="1.0" encoding="UTF-8"?>\n<BlockMap xmlns="http://schemas.microsoft.com/appx/2010/blockmap" HashMethod="http://www.w3.org/2001/04/xmlenc#sha256">']
    for name, data in files.items():
        blocks.append('<File Name="%s" Size="%d" LfhSize="30">' % (name.replace("/", "\\"), len(data)))
        for i in range(0, max(len(data), 1), 65536):
            h = base64.b64encode(hashlib.sha256(data[i:i + 65536]).digest()).decode()
            blocks.append('<Block Hash="%s" Size="1"/>' % h)
        blocks.append("</File>")
    blocks.append("</BlockMap>")
    if tamper:
        data = bytearray(files["bin/hello.exe"]); data[100] ^= 0xFF; files["bin/hello.exe"] = bytes(data)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in files.items():
            z.writestr(name.replace(" ", "%20"), data)
        z.writestr("AppxBlockMap.xml", "\n".join(blocks))
        z.writestr("[Content_Types].xml", "<Types/>")
package(work + "/test.msix", "x64")
package(work + "/altere.msix", "x64", tamper=True)
package(work + "/inner_x64.msix", "x64")
package(work + "/inner_x86.msix", "x86")
with zipfile.ZipFile(work + "/test.msixbundle", "w", zipfile.ZIP_STORED) as z:
    z.writestr("AppxMetadata/AppxBundleManifest.xml", """<Bundle><Packages>
      <Package Type="application" Architecture="x86" FileName="inner_x86.msix"/>
      <Package Type="application" Architecture="x64" FileName="inner_x64.msix"/>
    </Packages></Bundle>""")
    z.write(work + "/inner_x86.msix", "inner_x86.msix")
    z.write(work + "/inner_x64.msix", "inner_x64.msix")
PY

fail() { echo "msix : $*" >&2; exit 1; }

# Paquet modifié : refusé, rien d'installé.
if "$runtime" --setup "$work/altere.msix" >"$work/altere.txt" 2>&1; then
    fail "un paquet modifié a été installé"
fi
grep -q "altéré" "$work/altere.txt" || { cat "$work/altere.txt"; fail "message « paquet altéré » absent"; }
find "$home" -type d -name "WindowsApps" | grep -q . && fail "un paquet modifié a laissé des fichiers"
echo "msix : paquet modifié refusé, rien d'écrit"

"$runtime" --setup --info "$work/test.msix" >"$work/info.txt" || fail "--info a échoué"
grep -q "^msix	Test MSIX Winlux	1.0.0.0	Winlux	C:\\\\Program Files\\\\WindowsApps\\\\Winlux.TestMsix_1.0.0.0_x64__" "$work/info.txt" \
    || { cat "$work/info.txt"; fail "--info inattendu"; }

"$runtime" --setup "$work/test.msix" || fail "installation échouée"
full=$("$runtime" --setup --list | awk -F '\t' '$4 == "msix" { print $1 }')
case "$full" in Winlux.TestMsix_1.0.0.0_x64__*) ;; *) fail "paquet absent de --list : $full" ;; esac
family="Winlux.TestMsix_${full##*__}"
dir=$(find "$home" -type d -path "*WindowsApps/$full" | head -n 1)
test -n "$dir" || fail "dossier WindowsApps/$full absent"
test -f "$dir/bin/hello.exe" || fail "bin/hello.exe absent"
test -f "$dir/Assets/Logo grand.txt" || fail "nom encodé %20 mal décodé"
test -f "$dir/AppxManifest.xml" || fail "AppxManifest.xml non conservé"
test ! -e "$dir/[Content_Types].xml" || fail "[Content_Types].xml copié"
find "$home" -name "Bonjour MSIX.lnk" | grep -q . || fail "raccourci du menu Démarrer absent"
find "$home" -type d -path "*Packages/$family/LocalState" | grep -q . || fail "LocalState absent"
"$runtime" --reg query "HKCR\\.wlxdoc\\OpenWithProgids" >"$work/assoc.txt" 2>&1 || true
grep -qi "appx.winlux.testmsix.wlxdoc" "$work/assoc.txt" || { cat "$work/assoc.txt"; fail "association .wlxdoc absente"; }
"$runtime" --reg query "HKCR\\winlux-msix" >"$work/proto.txt" 2>&1 || true
grep -q "URL Protocol" "$work/proto.txt" || { cat "$work/proto.txt"; fail "protocole winlux-msix absent"; }
"$runtime" --setup --programs | grep -q "Test MSIX Winlux" || fail "absent des applications installées"
echo "msix : installé dans WindowsApps, raccourci, association, protocole, LocalState"

out=$("$runtime" "$dir/bin/hello.exe" | head -n 1)
test "$out" = "Bonjour depuis un .exe Windows, execute par Winlux !" || fail "le programme installé ne se lance pas : $out"
echo "msix : le programme installé se lance"

"$runtime" --setup --uninstall "$full" || fail "désinstallation échouée"
test ! -e "$dir/bin/hello.exe" || fail "fichiers restants après désinstallation"
find "$home" -name "Bonjour MSIX.lnk" | grep -q . && fail "raccourci restant"
"$runtime" --setup --list | grep -q "$full" && fail "paquet encore inscrit"
echo "msix : désinstallation complète"

"$runtime" --setup "$work/test.msixbundle" || fail "installation du bundle échouée"
"$runtime" --setup --list | awk -F '\t' '$4 == "msix" { print $1 }' | grep -q "_x64__" || fail "le bundle n'a pas installé le paquet x64"
echo "msix : bundle, paquet x64 choisi"
