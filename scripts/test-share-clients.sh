#!/bin/sh
# Partage de dossiers : un vrai serveur Samba face à des clients qui se
# comportent comme ceux de Windows 11 et de macOS (issue #171).
#
# - Windows 11 (24H2 et après) : SMB 3.1.1, signature SMB obligatoire, NTLMv2,
#   accès invité refusé ; noms accentués et longs, fichier ouvert puis relu.
# - macOS (Finder) : SMB 3, flux nommés AFP_AfpInfo / AFP_Resource (vfs fruit),
#   pas de fichiers ._ laissés sur le disque.
# - Découverte : sonde WS-Discovery (UDP 3702) puis métadonnées HTTP (5357),
#   exactement ce que fait le panneau Réseau de l'Explorateur Windows.
#
# Il faut root (comptes système de test, smbd) et Samba installé ; sinon le
# test est ignoré. Rien d'autre que la racine temporaire n'est modifié, à part
# deux comptes système wlxalice et wlxbob supprimés à la fin.
set -eu
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
if test "$(id -u)" != 0 || ! command -v smbclient >/dev/null 2>&1 || \
   { ! command -v smbd >/dev/null 2>&1 && ! test -x /usr/sbin/smbd; }; then
    echo "Partage face aux clients Windows/Mac : ignoré (root, smbd et smbclient requis)"
    exit 0
fi
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-share-clients.XXXXXX")
chmod 755 "$work"
root="$work/root"
port=${WINLUX_SHARE_PORT:-44545}
export WINLUX_CONTROL_ROOT="$root" WINLUX_SHARE_PORT="$port" HOME="$root/home/wlxalice"
cleanup() {
    WINLUX_SHARE_AVAILABLE=1 sh "$project/scripts/winlux-share.sh" disable >/dev/null 2>&1 || true
    for u in wlxalice wlxbob; do userdel "$u" >/dev/null 2>&1 || true; done
    rm -rf "$work"
}
trap cleanup EXIT HUP INT TERM
share() { sh "$project/scripts/winlux-share.sh" "$@" >/dev/null; }
state="$root/run/winlux/shares.txt"
fail() {
    echo "ÉCHEC : $*" >&2
    cat "$work/smb.log" >&2 2>/dev/null || true
    cat "$root/etc/winlux/samba/smb.conf" "$root"/run/winlux/samba/log.* 2>/dev/null | tail -n 60 >&2 || true
    exit 1
}
for u in wlxalice wlxbob; do
    id -u "$u" >/dev/null 2>&1 || useradd -M -d /nonexistent -s /usr/sbin/nologin "$u"
done
mkdir -p "$root/etc/winlux" "$root/etc/avahi/services" "$HOME/Photos été"
printf 'wlxalice:user:Motdepasse-1\nwlxbob:user:Autre-2\n' > "$root/etc/winlux/users.conf"
chown -R wlxalice "$HOME"
share add "$HOME/Photos été"
share set 1 wlxbob read
share status
grep -qx 'running=1' "$state" || fail "serveur non démarré"

host=127.0.0.1
# Client Windows 11 : SMB 3.1.1 seulement, signature exigée, NTLMv2.
win() {
    smbclient "//$host/Photos été" -p "$port" -m SMB3_11 --option='client min protocol=SMB3_11' \
        --client-protection=sign "$@"
}
mac() {
    smbclient "//$host/Photos été" -p "$port" -m SMB3 --option='client min protocol=SMB3' "$@"
}

smbclient -L "$host" -p "$port" -m SMB3_11 --client-protection=sign -U 'wlxalice%Motdepasse-1' 2>/dev/null \
    | grep -q 'Photos été' || fail "Windows 11 : le partage n'apparaît pas dans la liste"
printf 'Bonjour depuis Windows\r\n' > "$work/note.txt"
long=$(printf 'Dossier très long %.0s' 1 2 3 4 5 6 7 8 9 10 11 12)
# Windows interdit l'espace final dans un nom (Samba récent aussi).
long=${long% }
win -U 'wlxalice%Motdepasse-1' -c "mkdir \"$long\"; put \"$work/note.txt\" \"$long/Résumé à lire.txt\"" >"$work/smb.log" 2>&1 \
    || fail "Windows 11 : écriture refusée au propriétaire"
test "$(stat -c %U "$HOME/Photos été/$long/Résumé à lire.txt")" = wlxalice || fail "fichier écrit sous un autre propriétaire"
win -U 'wlxbob%Autre-2' -c "get \"$long/Résumé à lire.txt\" \"$work/relu.txt\"" >"$work/smb.log" 2>&1 \
    || fail "Windows 11 : lecture refusée à un compte en lecture"
cmp -s "$work/note.txt" "$work/relu.txt" || fail "Windows 11 : contenu relu différent"
win -U 'wlxbob%Autre-2' -c "put \"$work/note.txt\" interdit.txt" >/dev/null 2>&1 || true
test ! -e "$HOME/Photos été/interdit.txt" || fail "Windows 11 : écriture acceptée en lecture seule"
win -U 'wlxalice%mauvais' -c ls >/dev/null 2>&1 && fail "Windows 11 : mauvais mot de passe accepté"
smbclient -L "$host" -p "$port" -m NT1 --option='client max protocol=NT1' -N >/dev/null 2>&1 \
    && fail "SMB1 accepté"

# Invités : un Mac y accède ; Windows 11 exige la signature et refuse les
# invités, ce que la page Paramètres signale.
share guest 1 read
mac -N -c ls >/dev/null 2>&1 || fail "Mac : accès invité refusé alors qu'il est ouvert"
win -N -c ls >/dev/null 2>&1 && fail "Windows 11 : session invitée signée inattendue (comportement à revoir dans l'interface)"
share guest 1 none

# Finder : métadonnées et fourche de ressources en flux nommés.
printf 'icône' > "$work/meta"
mac -U 'wlxalice%Motdepasse-1' -c "put \"$work/note.txt\" Mac.txt; put \"$work/meta\" \"Mac.txt:AFP_Resource\"; allinfo Mac.txt" \
    > "$work/mac.log" 2>&1 || { cat "$work/mac.log" >&2; fail "Mac : écriture des flux refusée"; }
if grep -q "vfs objects = .*fruit" "$root/etc/winlux/samba/smb.conf"; then
    grep -q 'AFP_Resource' "$work/mac.log" || { cat "$work/mac.log" >&2; fail "Mac : fourche de ressources perdue"; }
fi
ls -a "$HOME/Photos été" | grep -q '^\._' && fail "Mac : fichiers ._ laissés sur le disque"

# Découverte Windows (WS-Discovery), si wsdd est installé.
if command -v wsdd >/dev/null 2>&1; then
    python3 -I - "$(hostname -I 2>/dev/null | cut -d' ' -f1)" <<'PY' || fail "Windows : PC introuvable par WS-Discovery"
import socket, sys, time, uuid, urllib.request
addr = sys.argv[1] or "127.0.0.1"
probe = f'''<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope" xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:wsd="http://schemas.xmlsoap.org/ws/2005/04/discovery" xmlns:wsdp="http://schemas.xmlsoap.org/ws/2006/02/devprof">
<soap:Header><wsa:To>urn:schemas-xmlsoap-org:ws:2005:04:discovery</wsa:To><wsa:Action>http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</wsa:Action><wsa:MessageID>urn:uuid:{uuid.uuid4()}</wsa:MessageID></soap:Header>
<soap:Body><wsd:Probe><wsd:Types>wsdp:Device</wsd:Types></wsd:Probe></soap:Body></soap:Envelope>'''.encode()
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.settimeout(1)
for _ in range(10):
    for target in (("239.255.255.250", 3702), (addr, 3702)):
        try: sock.sendto(probe, target)
        except OSError: pass
    try:
        data, _ = sock.recvfrom(65536)
    except socket.timeout:
        continue
    if b"ProbeMatches" in data:
        break
else:
    sys.exit("pas de ProbeMatches")
endpoint = data.split(b"<wsa:Address>", 1)[1].split(b"</wsa:Address>", 1)[0].decode()
get = f'''<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope" xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing">
<soap:Header><wsa:To>{endpoint}</wsa:To><wsa:Action>http://schemas.xmlsoap.org/ws/2004/09/transfer/Get</wsa:Action><wsa:MessageID>urn:uuid:{uuid.uuid4()}</wsa:MessageID><wsa:ReplyTo><wsa:Address>http://schemas.xmlsoap.org/ws/2004/08/addressing/role/anonymous</wsa:Address></wsa:ReplyTo></soap:Header>
<soap:Body/></soap:Envelope>'''.encode()
url = f"http://{addr}:5357/{endpoint.split(':')[-1]}"
req = urllib.request.Request(url, data=get, headers={"Content-Type": "application/soap+xml"})
meta = urllib.request.urlopen(req, timeout=5).read()
if b"Computer" not in meta or b"WORKGROUP" not in meta.upper():
    sys.exit("métadonnées sans nom de PC ni groupe de travail")
PY
fi
test -f "$root/etc/avahi/services/winlux-smb.service" || fail "Mac : service Bonjour non annoncé"

share disable
grep -qx 'running=0' "$state" || fail "serveur encore actif après désactivation"
echo "Partage face aux clients Windows 11 et macOS : OK"
