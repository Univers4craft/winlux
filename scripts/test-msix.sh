#!/bin/sh
# Paquets MSIX (#272) : construit un petit .msix (manifeste, block map,
# hello.exe) et un .msixbundle, puis vérifie sous Winlux
#   - l'installation dans C:\Program Files\WindowsApps\<PackageFullName> ;
#   - le raccourci du menu Démarrer, l'association .wlxdoc et le protocole ;
#   - l'entrée de « Applications installées » et le dossier LocalState ;
#   - que le programme installé se lance ;
#   - le refus par défaut d'un éditeur non vérifié (non signé, ou signé
#     par une autorité inconnue), code 3, et l'installation après
#     confirmation (--allow-unverified) ;
#   - avec la commande openssl (#235) : l'installation sans question d'un
#     paquet signé par une autorité reconnue (SSL_CERT_FILE), et le refus
#     sans recours d'une signature au nom d'un autre éditeur, d'une
#     signature fausse, d'un paquet modifié après signature et d'une
#     signature sans signataire ;
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
import base64, hashlib, io, shutil, struct, subprocess, sys, zipfile, zlib
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
def der(tag, body):
    n = len(body)
    head = bytes([n]) if n < 0x80 else bytes([0x82, n >> 8, n & 0xFF])
    return bytes([tag]) + head + body
def fake_p7x(cn):
    # Certificat seul, sans signataire : refusé.
    cn_oid = der(0x06, b"\x55\x04\x03")
    name = lambda v: der(0x30, der(0x31, der(0x30, cn_oid + der(0x0C, v.encode()))))
    tbs = der(0xA0, der(0x02, b"\x02")) + der(0x02, b"\x01") + der(0x30, b"") + name("Autorite de test") \
        + der(0x30, b"") + name(cn) + der(0x30, b"")
    cert = der(0x30, der(0x30, tbs) + der(0x30, b"") + der(0x03, b"\x00"))
    signed = der(0x02, b"\x01") + der(0x31, b"") + der(0x30, b"") + der(0xA0, cert) + der(0x31, b"")
    oid = der(0x06, bytes([0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02]))
    return b"PKCX" + der(0x30, oid + der(0xA0, der(0x30, signed)))
def contents(arch):
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
    return files, "\n".join(blocks).encode()
def package(path, arch, tamper=False, signer=None):
    files, blockmap = contents(arch)
    if tamper:
        data = bytearray(files["bin/hello.exe"]); data[100] ^= 0xFF; files["bin/hello.exe"] = bytes(data)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in files.items():
            z.writestr(name.replace(" ", "%20"), data)
        z.writestr("AppxBlockMap.xml", blockmap)
        z.writestr("[Content_Types].xml", "<Types/>")
        if signer:
            z.writestr("AppxSignature.p7x", fake_p7x(signer))

# Vraie signature (Authenticode CMS) avec la commande openssl : condensats
# APPX du paquet non signé, puis AppxSignature.p7x ajouté en dernière
# entrée, comme le fait l'outil de signature de Windows.
def unsigned(arch, blockmap_override=None):
    files, blockmap = contents(arch)
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in files.items():
            z.writestr(name.replace(" ", "%20"), data)
        z.writestr("AppxBlockMap.xml", blockmap_override or blockmap)
        z.writestr("[Content_Types].xml", "<Types/>")
    return buf.getvalue(), blockmap
def openssl(*args):
    subprocess.run(["openssl", *args], check=True, cwd=work, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
def make_keys():
    open(work + "/leaf.ext", "w").write("basicConstraints=CA:FALSE\nkeyUsage=digitalSignature\nextendedKeyUsage=codeSigning\n")
    openssl("req", "-x509", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=Autorite de test Winlux", "-days", "2",
            "-keyout", "ca.key", "-out", "ca.pem", "-addext", "basicConstraints=critical,CA:TRUE",
            "-addext", "keyUsage=critical,keyCertSign")
    for leaf, cn in (("leaf", "Winlux Test"), ("autre", "Autre editeur")):
        openssl("req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=" + cn, "-keyout", leaf + ".key", "-out", leaf + ".csr")
        openssl("x509", "-req", "-in", leaf + ".csr", "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial",
                "-days", "2", "-out", leaf + ".pem", "-extfile", "leaf.ext")
def signed(path, leaf="leaf", blockmap_signed=None, corrupt=False):
    data, blockmap = unsigned("x64")
    eocd = data.rindex(b"PK\x05\x06")
    entries, cd_size, cd_off = struct.unpack("<HII", data[eocd + 10:eocd + 20])
    sha = lambda b: hashlib.sha256(b).digest()
    digests = b"APPX" + b"AXPC" + sha(data[:cd_off]) + b"AXCD" + sha(data[cd_off:]) \
        + b"AXCT" + sha(b"<Types/>") + b"AXBM" + sha(blockmap_signed or blockmap)
    sha256 = der(0x30, der(0x06, bytes.fromhex("608648016503040201")) + der(0x05, b""))
    spc = der(0x30, der(0x30, der(0x06, bytes.fromhex("2b060104018237021e"))) + der(0x30, sha256 + der(0x04, digests)))
    open(work + "/spc.der", "wb").write(spc)
    openssl("cms", "-sign", "-binary", "-nodetach", "-outform", "DER", "-md", "sha256",
            "-econtent_type", "1.3.6.1.4.1.311.2.1.4", "-in", "spc.der", "-signer", leaf + ".pem",
            "-inkey", leaf + ".key", "-certfile", "ca.pem", "-nosmimecap", "-out", "sig.der")
    sig = bytearray(b"PKCX" + open(work + "/sig.der", "rb").read())
    if corrupt:
        sig[-1] ^= 0x01  # dernier octet de la valeur de signature
    name = b"AppxSignature.p7x"
    crc = zlib.crc32(sig)
    local = struct.pack("<IHHHHHIIIHH", 0x04034B50, 20, 0, 0, 0, 0x21, crc, len(sig), len(sig), len(name), 0) + name + sig
    central = struct.pack("<IHHHHHHIIIHHHHHII", 0x02014B50, 20, 20, 0, 0, 0, 0x21, crc, len(sig), len(sig),
                          len(name), 0, 0, 0, 0, 0, cd_off) + name
    cd = data[cd_off:cd_off + cd_size] + central
    end = struct.pack("<IHHHHIIH", 0x06054B50, 0, 0, entries + 1, entries + 1, len(cd), cd_off + len(local), 0)
    open(path, "wb").write(data[:cd_off] + local + cd + end)

package(work + "/test.msix", "x64")
package(work + "/sans-signataire.msix", "x64", signer="Winlux Test")
if shutil.which("openssl"):
    make_keys()
    signed(work + "/signe.msix")
    signed(work + "/autre-editeur.msix", leaf="autre")
    signed(work + "/falsifie.msix", corrupt=True)
    signed(work + "/modifie-apres.msix", blockmap_signed=b"<BlockMap/>")
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

nothing_installed() { ! find "$home" -type d -name "WindowsApps" | grep -q .; }

# Éditeur non vérifié : refus par défaut (code 3), rien d'écrit.
set +e
"$runtime" --setup "$work/test.msix" >"$work/nonsigne.txt" 2>&1; rc=$?
set -e
test "$rc" -eq 3 || { cat "$work/nonsigne.txt"; fail "paquet non signé : code $rc au lieu de 3"; }
grep -q "Éditeur non vérifié : ce paquet n'est pas signé" "$work/nonsigne.txt" || { cat "$work/nonsigne.txt"; fail "avertissement « non signé » absent"; }
nothing_installed || fail "un paquet non signé a été installé sans confirmation"
# Double-clic sans affichage pour confirmer : refus, rien d'installé.
set +e
env -u DISPLAY -u WAYLAND_DISPLAY WINLUX_RUNTIME="$runtime" XDG_STATE_HOME="$work/state" \
    sh "$root/integration/winlux-open" "$work/test.msix" >"$work/open.txt" 2>&1; rc=$?
set -e
test "$rc" -eq 3 || { cat "$work/open.txt"; fail "double-clic sans confirmation : code $rc au lieu de 3"; }
nothing_installed || fail "le double-clic a installé un paquet non vérifié sans confirmation"
echo "msix : éditeur non vérifié refusé par défaut (non signé, double-clic)"

refused() { # paquet, motif attendu : refus même avec confirmation
    if "$runtime" --setup --allow-unverified "$work/$1" >"$work/refus.txt" 2>&1; then
        fail "$1 a été installé"
    fi
    grep -q "signature refusée" "$work/refus.txt" && grep -q "$2" "$work/refus.txt" \
        || { cat "$work/refus.txt"; fail "$1 : message « $2 » absent"; }
    nothing_installed || fail "$1 a laissé des fichiers"
}
refused sans-signataire.msix "AppxSignature.p7x illisible"
echo "msix : signature sans signataire refusée"

if test -f "$work/signe.msix"; then
    mkdir -p "$work/aucune-autorite"
    : >"$work/aucune-autorite.pem"
    # Autorité inconnue du système : éditeur non vérifié, code 3.
    set +e
    SSL_CERT_FILE="$work/aucune-autorite.pem" SSL_CERT_DIR="$work/aucune-autorite" \
        "$runtime" --setup "$work/signe.msix" >"$work/signe.txt" 2>&1; rc=$?
    set -e
    test "$rc" -eq 3 || { cat "$work/signe.txt"; fail "autorité inconnue : code $rc au lieu de 3"; }
    grep -q "signé au nom de « CN=Winlux Test », mais son certificat ne vient pas d'une autorité reconnue" "$work/signe.txt" \
        || { cat "$work/signe.txt"; fail "avertissement « autorité inconnue » absent"; }
    nothing_installed || fail "un paquet d'une autorité inconnue a été installé sans confirmation"
    # Autorité reconnue : installé sans question.
    SSL_CERT_FILE="$work/ca.pem" SSL_CERT_DIR="$work/aucune-autorite" \
        "$runtime" --setup "$work/signe.msix" >"$work/verifie.txt" 2>&1 \
        || { cat "$work/verifie.txt"; fail "paquet signé par une autorité reconnue refusé"; }
    full=$("$runtime" --setup --list | awk -F '\t' '$4 == "msix" { print $1 }')
    test -n "$full" || fail "paquet vérifié absent de --list"
    "$runtime" --setup --uninstall "$full" || fail "désinstallation du paquet vérifié échouée"
    echo "msix : signature vérifiée, installé sans question ; autorité inconnue à confirmer"
    export SSL_CERT_FILE="$work/ca.pem" SSL_CERT_DIR="$work/aucune-autorite"
    refused autre-editeur.msix "au nom de « CN=Autre editeur » et non de l'éditeur"
    refused falsifie.msix "signature fausse"
    refused modifie-apres.msix "AppxBlockMap.xml a été modifié après la signature"
    unset SSL_CERT_FILE SSL_CERT_DIR
    echo "msix : autre éditeur, signature fausse, paquet modifié après signature refusés"
else
    echo "msix : commande openssl absente, vraies signatures non testées"
fi

# Paquet modifié : refusé, rien d'installé.
if "$runtime" --setup --allow-unverified "$work/altere.msix" >"$work/altere.txt" 2>&1; then
    fail "un paquet modifié a été installé"
fi
grep -q "altéré" "$work/altere.txt" || { cat "$work/altere.txt"; fail "message « paquet altéré » absent"; }
find "$home" -type d -name "WindowsApps" | grep -q . && fail "un paquet modifié a laissé des fichiers"
echo "msix : paquet modifié refusé, rien d'écrit"

"$runtime" --setup --info "$work/test.msix" >"$work/info.txt" || fail "--info a échoué"
grep -q "^msix	Test MSIX Winlux	1.0.0.0	Winlux	C:\\\\Program Files\\\\WindowsApps\\\\Winlux.TestMsix_1.0.0.0_x64__" "$work/info.txt" \
    || { cat "$work/info.txt"; fail "--info inattendu"; }

"$runtime" --setup --allow-unverified "$work/test.msix" || fail "installation échouée"
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

"$runtime" --setup --allow-unverified "$work/test.msixbundle" || fail "installation du bundle échouée"
"$runtime" --setup --list | awk -F '\t' '$4 == "msix" { print $1 }' | grep -q "_x64__" || fail "le bundle n'a pas installé le paquet x64"
echo "msix : bundle, paquet x64 choisi"
