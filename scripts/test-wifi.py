#!/usr/bin/python3
"""Page Wi-Fi de Paramètres et service winlux-wifi, avec un faux NetworkManager.

  python3 -B scripts/test-wifi.py                  service, relais root, traductions
  xvfb-run -a python3 -B scripts/test-wifi.py      + la page GTK : liste, boîte de
                                                   la clé, clavier tactile, erreurs

Le faux nmcli garde son état dans un dossier temporaire et note chaque appel :
le test vérifie que la clé n'apparaît jamais dans une ligne de commande, qu'une
clé refusée n'est pas enregistrée, et que chaque erreur est dite en clair.
"""
from importlib.machinery import SourceFileLoader
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
WIFI = HERE / "winlux-wifi.py"
GOOD_KEY = "bonne-cle-123"
failures = []


def ok(condition, label):
    print(("ok   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


FAKE_NMCLI = r'''#!/usr/bin/python3
import json, os, re, sys
from pathlib import Path
state_dir = Path(os.environ["FAKE_NM_STATE"])
connections = Path(os.environ["WINLUX_WIFI_CONNECTIONS"])
args = sys.argv[1:]
with open(state_dir / "calls.log", "a", encoding="utf-8") as log:
    log.write(json.dumps(args) + "\n")
state_file = state_dir / "state.json"
state = json.loads(state_file.read_text()) if state_file.exists() else {"radio": "enabled", "active": ""}
def save(): state_file.write_text(json.dumps(state))
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
def esc(text): return text.replace("\\", "\\\\").replace(":", "\\:")
def ssid_of(values):
    raw = values.get("wifi.ssid", "")
    if re.fullmatch(r"([0-9]+;)+", raw): return bytes(int(b) for b in raw.split(";") if b).decode()
    return raw
NETWORKS = [("Maison:5G", 82, "WPA2"), ("Maison:5G", 40, "WPA2"), ("Café", 55, ""), ("Voisin", 20, "WPA3"),
            ("Bureau", 64, "WPA2 802.1X"), ("Hors portée", 30, "WPA2")]
joined = " ".join(args)
if joined == "-t -f RUNNING general": print("running")
elif joined == "-t -f DEVICE,TYPE,STATE device": print("wlan0:wifi:disconnected\neth0:ethernet:unavailable")
elif joined == "-t -f WIFI radio": print(state["radio"])
elif args[:2] == ["radio", "wifi"]: state["radio"] = "enabled" if args[2] == "on" else "disabled"; save()
elif args[:2] == ["radio", "all"]: state["radio"] = "enabled" if args[2] == "on" else "disabled"; save()
elif "device wifi list" in joined:
    for ssid, signal, security in NETWORKS:
        print(":".join(["*" if ssid == state["active"] else " ", esc(ssid), str(signal), security or "--", esc("AA:BB:CC:DD:EE:FF")]))
elif joined == "-t -f UUID,TYPE,AUTOCONNECT,ACTIVE,NAME connection show":
    for _, values in profiles():
        auto = "no" if values.get("connection.autoconnect") == "false" else "yes"
        active = "yes" if ssid_of(values) == state["active"] else "no"
        print(":".join([values["connection.uuid"], "802-11-wireless", auto, active, esc(values["connection.id"])]))
elif args[:2] == ["-g", "802-11-wireless.ssid"]:
    for _, values in profiles():
        if values["connection.uuid"] == args[-1]: print(esc(ssid_of(values)))
elif args[:2] == ["connection", "load"]: pass
elif "connection up uuid" in joined:
    target = args[args.index("uuid") + 1]
    for _, values in profiles():
        if values["connection.uuid"] != target: continue
        ssid = ssid_of(values)
        if ssid == "Hors portée":
            print("Error: Connection activation failed: No network with SSID 'Hors portée' found.", file=sys.stderr); sys.exit(10)
        secured = values.get("wifi-security.key-mgmt") in ("wpa-psk", "sae")
        if secured and values.get("wifi-security.psk") != "bonne-cle-123":
            print("Error: Connection activation failed: (7) Secrets were required, but not provided.", file=sys.stderr); sys.exit(4)
        state["active"] = ssid; save(); print("Connection successfully activated"); sys.exit(0)
    print("Error: unknown connection", file=sys.stderr); sys.exit(10)
elif args[:3] == ["connection", "delete", "uuid"]:
    for path, values in profiles():
        if values["connection.uuid"] == args[3]:
            if ssid_of(values) == state["active"]: state["active"] = ""; save()
            path.unlink()
elif args[:3] == ["connection", "modify", "uuid"]:
    for path, values in profiles():
        if values["connection.uuid"] == args[3]:
            text = path.read_text().replace("autoconnect=true", "autoconnect=" + ("true" if args[5] == "yes" else "false"))
            path.write_text(text)
elif args[:2] == ["device", "disconnect"]: state["active"] = ""; save()
else:
    print("fake nmcli: " + joined, file=sys.stderr); sys.exit(2)
'''


def setup():
    work = Path(tempfile.mkdtemp(prefix="winlux-wifi-"))
    (work / "bin").mkdir()
    (work / "connections").mkdir()
    nmcli = work / "bin/nmcli"
    nmcli.write_text(FAKE_NMCLI)
    nmcli.chmod(0o755)
    sys_root = work / "sys-root"
    for directory in ("sys/class/ieee80211/phy0", "sys/class/net/wlan0/wireless", "sys/class/rfkill/rfkill0",
                      "sys/class/rfkill/rfkill1"):
        (sys_root / directory).mkdir(parents=True)
    (sys_root / "sys/class/rfkill/rfkill0/type").write_text("wlan\n")
    (sys_root / "sys/class/rfkill/rfkill0/soft").write_text("0\n")
    (sys_root / "sys/class/rfkill/rfkill1/type").write_text("bluetooth\n")
    (sys_root / "sys/class/rfkill/rfkill1/soft").write_text("0\n")
    env = dict(os.environ, PATH=f"{work / 'bin'}:{os.environ.get('PATH', '/usr/bin:/bin')}",
               FAKE_NM_STATE=str(work), WINLUX_WIFI_CONNECTIONS=str(work / "connections"),
               WINLUX_SYS_ROOT=str(sys_root), WINLUX_WIFI_DIRECT="1", WINLUX_LANG="fr",
               XDG_CONFIG_HOME=str(work / "config"), WINLUX_AIRPLANE_MARK=str(work / "airplane-mark"),
               WINLUX_WIFI_CERTS=str(work / "certificats-gardes"))
    return work, env


def backend(env, *args, key=None):
    result = subprocess.run([sys.executable, "-B", str(WIFI), *args], input=(key + "\n") if key is not None else "",
                            capture_output=True, text=True, env=env, timeout=60)
    try:
        return json.loads(result.stdout.strip().splitlines()[-1])
    except (IndexError, ValueError):
        return {"ok": False, "error": "no-json", "detail": result.stdout + result.stderr}


def load(name, path, lang="fr"):
    os.environ["WINLUX_LANG"] = lang
    loader = SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def test_backend():
    work, env = setup()
    calls = work / "calls.log"
    connections = work / "connections"
    status = backend(env, "status")
    ok(status.get("ok") and status["present"] and status["radio"] and status["device"] == "wlan0"
       and status["connected"] == "", "état : carte présente, Wi-Fi activé, non connecté")
    listing = backend(env, "list", "--rescan")
    names = [n["ssid"] for n in listing.get("networks", [])]
    ok(names.count("Maison:5G") == 1 and names[0] == "Maison:5G", "liste : un réseau par nom, le plus fort d'abord, « : » rétabli")
    ok(listing["networks"][0]["signal"] == 82, "liste : deux bornes du même réseau → le meilleur signal")
    kinds = {n["ssid"]: n["security"] for n in listing["networks"]}
    ok(kinds == {"Maison:5G": "wpa", "Café": "open", "Voisin": "sae", "Bureau": "eap", "Hors portée": "wpa"},
       f"liste : types de sécurité reconnus ({kinds})")
    ok(any("--rescan" in json.loads(line) for line in calls.read_text().splitlines()), "Actualiser relance une vraie recherche")

    wrong = backend(env, "connect", "Maison:5G", "--security", "wpa", "--key-stdin", key="mauvaise-cle")
    ok(wrong.get("error") == "bad-key", f"clé refusée → « bad-key » ({wrong})")
    ok(not list(connections.glob("*.nmconnection")), "clé refusée : rien n'est enregistré")
    short = backend(env, "connect", "Maison:5G", "--security", "wpa", "--key-stdin", key="court")
    ok(short.get("error") == "key-format", "clé WPA de moins de 8 caractères refusée avant d'appeler la box")

    good = backend(env, "connect", "Maison:5G", "--security", "wpa", "--key-stdin", key=GOOD_KEY)
    files = list(connections.glob("*.nmconnection"))
    ok(good.get("ok") and len(files) == 1, f"bonne clé : connecté et réseau enregistré ({good})")
    if files:
        text = files[0].read_text()
        ok(oct(files[0].stat().st_mode & 0o777) == "0o600", "profil enregistré en 0600 (clé lisible par root seul)")
        ok(f"psk={GOOD_KEY}" in text and "autoconnect=true" in text and "key-mgmt=wpa-psk" in text,
           "profil : clé WPA et reconnexion automatique au démarrage")
        ok("permissions=" not in text or "permissions=\n" in text, "profil système : il sert à tous les comptes, avant la session")
    ok(GOOD_KEY not in calls.read_text() and "mauvaise-cle" not in calls.read_text(),
       "la clé ne passe jamais en argument de nmcli (visible dans ps)")
    status = backend(env, "status")
    ok(status.get("connected") == "Maison:5G", "état : connecté à Maison:5G")
    listing = backend(env, "list")
    known = {n["ssid"]: n["known"] for n in listing["networks"]}
    ok(known["Maison:5G"] and not known["Café"], "liste : réseau enregistré signalé")

    again = backend(env, "connect", "Maison:5G", "--security", "wpa", "--key-stdin", key=GOOD_KEY)
    ok(again.get("ok") and len(list(connections.glob("*.nmconnection"))) == 1, "nouvelle clé : l'ancien profil est remplacé, pas doublé")
    up = backend(env, "up", "Maison:5G")
    ok(up.get("ok"), "réseau enregistré : connexion sans redemander la clé")

    off = backend(env, "autoconnect", "Maison:5G", "off")
    ok(off.get("ok") and "autoconnect=false" in next(connections.glob("*.nmconnection")).read_text(),
       "« Se connecter automatiquement » décoché")
    gone = backend(env, "connect", "Hors portée", "--security", "wpa", "--key-stdin", key=GOOD_KEY)
    ok(gone.get("error") == "out-of-range", f"réseau hors de portée dit en clair ({gone})")
    ok(len(list(connections.glob("*.nmconnection"))) == 1, "réseau hors de portée : pas de profil laissé")

    hidden = backend(env, "connect", "Caché;1", "--security", "wpa", "--hidden", "--key-stdin", key=GOOD_KEY)
    hidden_files = [p for p in connections.glob("*.nmconnection") if "hidden=true" in p.read_text()]
    ok(hidden.get("ok") and len(hidden_files) == 1, f"réseau caché : profil « hidden=true » ({hidden})")
    if hidden_files:
        ok("ssid=" + "".join(f"{b};" for b in "Caché;1".encode()) in hidden_files[0].read_text(),
           "nom de réseau avec « ; » écrit octet par octet (relu tel quel par NetworkManager)")
    open_net = backend(env, "connect", "Café", "--security", "open")
    ok(open_net.get("ok"), "réseau ouvert : connexion sans clé")
    enterprise = backend(env, "connect", "Bureau", "--security", "eap", "--key-stdin", key="mdp")
    ok(enterprise.get("error") == "eap-identity", "réseau d'entreprise : nom d'utilisateur demandé")

    def profile_of(ssid):
        for path in connections.glob("*.nmconnection"):
            text = path.read_text()
            if f"id={ssid}\n" in text: return text
        return ""
    # Entreprise PEAP/MSCHAPv2, serveur vérifié par les certificats du système.
    peap = backend(env, "connect", "Bureau", "--security", "eap", "--identity", "alice", "--eap", "peap",
                   "--phase2", "mschapv2", "--ca", "system", "--domain", "radius.exemple.fr", "--key-stdin", key="mdp-secret")
    text = profile_of("Bureau")
    ok(peap.get("ok") and "eap=peap;" in text and "phase2-auth=mschapv2" in text and "system-ca-certs=true" in text
       and "domain-suffix-match=radius.exemple.fr" in text and "password=mdp-secret" in text,
       f"Entreprise PEAP/MSCHAPv2 : profil complet ({peap})")
    ok("mdp-secret" not in calls.read_text(), "Entreprise : le mot de passe ne passe pas en argument")
    ttls = backend(env, "connect", "Bureau", "--security", "eap", "--identity", "alice", "--eap", "ttls",
                   "--phase2", "pap", "--ca", "none", "--key-stdin", key="mdp-secret")
    text = profile_of("Bureau")
    ok(ttls.get("ok") and "eap=ttls;" in text and "phase2-auth=pap" in text and "system-ca-certs" not in text
       and "ca-cert=" not in text, f"Entreprise TTLS/PAP sans vérification du serveur ({ttls})")
    certs = work / "certs"
    certs.mkdir(exist_ok=True)
    (certs / "alice.p12").write_bytes(b"p12")
    (certs / "ca.pem").write_text("-----BEGIN CERTIFICATE-----\n")
    tls = backend(env, "connect", "Bureau", "--security", "eap", "--identity", "alice", "--eap", "tls",
                  "--ca", str(certs / "ca.pem"), "--client-cert", str(certs / "alice.p12"), "--key-stdin", key="code-p12")
    text = profile_of("Bureau")
    kept = Path(env["WINLUX_WIFI_CERTS"])
    folders = [path for path in kept.iterdir()] if kept.is_dir() else []
    ok(len(folders) == 1, f"certificats copiés à l'abri, un dossier par profil ({folders})")
    copy = folders[0] if folders else kept
    ok(tls.get("ok") and "eap=tls;" in text and f"client-cert={copy / 'client-cert.p12'}" in text
       and f"private-key={copy / 'client-cert.p12'}" in text and "private-key-password=code-p12" in text
       and f"ca-cert={copy / 'ca.pem'}" in text and "password=" not in text.replace("private-key-password=", ""),
       f"Entreprise TLS : certificat client .p12, autorité choisie ({tls})")
    ok((copy / "client-cert.p12").read_bytes() == b"p12" and (copy / "client-cert.p12").stat().st_mode & 0o077 == 0,
       "copie du certificat personnel en 0600")
    (certs / "alice.p12").unlink()
    ok((copy / "client-cert.p12").is_file(), "le profil garde son certificat si l'original disparaît")
    (certs / "alice.p12").write_bytes(b"p12")
    no_cert = backend(env, "connect", "Bureau", "--security", "eap", "--identity", "alice", "--eap", "tls")
    ok(no_cert.get("error") == "eap-certificate", f"Entreprise TLS sans certificat : demandé en clair ({no_cert})")
    lost = backend(env, "connect", "Bureau", "--security", "eap", "--identity", "alice", "--eap", "tls",
                   "--client-cert", str(certs / "absent.p12"))
    ok(lost.get("error") == "eap-file", f"certificat introuvable dit en clair ({lost})")
    bad_method = backend(env, "connect", "Bureau", "--security", "eap", "--identity", "a", "--eap", "md5", "--key-stdin", key="x")
    ok(not bad_method.get("ok"), "méthode EAP inconnue refusée")
    # WPA3 personnel : SAE, trames de gestion protégées obligatoires.
    sae = backend(env, "connect", "Voisin", "--security", "sae", "--key-stdin", key=GOOD_KEY)
    text = profile_of("Voisin")
    ok(sae.get("ok") and "key-mgmt=sae" in text and "pmf=3" in text, f"WPA3 personnel : SAE et PMF obligatoire ({sae})")
    # WEP (vieux matériel) : clé de 13 caractères.
    wep = backend(env, "connect", "Café", "--security", "wep", "--key-stdin", key="abcdefghijklm")
    text = profile_of("Café")
    ok(wep.get("ok") and "key-mgmt=none" in text and "wep-key0=abcdefghijklm" in text, f"WEP : clé enregistrée ({wep})")
    for ssid in ("Bureau", "Voisin", "Café"):
        backend(env, "forget", ssid)
    backend(env, "connect", "Café", "--security", "open")

    forget = backend(env, "forget", "Maison:5G")
    ok(forget.get("ok") and forget.get("removed") == 1 and not any(
        "Maison" in p.read_text() for p in connections.glob("*.nmconnection")), "« Oublier » efface le profil et sa clé")

    # Couper le seul Wi-Fi n'est pas le mode avion (même sans Bluetooth).
    sys_root = Path(env["WINLUX_SYS_ROOT"])
    (sys_root / "sys/class/rfkill/rfkill1/soft").write_text("1\n")
    backend(env, "radio", "off")
    off_state = backend(env, "status")
    ok(off_state.get("radio") is False and off_state.get("airplane") is False,
       f"Wi-Fi coupé seul : le mode avion reste éteint ({off_state})")
    backend(env, "radio", "on")
    (sys_root / "sys/class/rfkill/rfkill1/soft").write_text("0\n")
    # En-tête : le profil actif compte, même si la liste n'est pas encore à jour.
    backend(env, "connect", "Voisin", "--security", "sae", "--key-stdin", key=GOOD_KEY)
    cafe_state = backend(env, "status")
    ok(cafe_state.get("connected") == "Voisin", f"état : réseau connecté d'après le profil actif ({cafe_state})")
    backend(env, "forget", "Voisin")
    airplane = backend(env, "airplane", "on")
    ok(airplane.get("ok") and (sys_root / "sys/class/rfkill/rfkill1/soft").read_text().strip() == "1"
       and '["radio", "all", "off"]' in calls.read_text(), "mode avion : Wi-Fi et Bluetooth coupés")
    ok(backend(env, "status").get("airplane") is True, "état : mode avion reconnu")
    blocked = backend(env, "connect", "Maison:5G", "--security", "wpa", "--key-stdin", key=GOOD_KEY)
    ok(blocked.get("error") == "wifi-off", "mode avion : « Wi-Fi désactivé » plutôt que « non autorisé »")
    backend(env, "airplane", "off")
    ok((sys_root / "sys/class/rfkill/rfkill1/soft").read_text().strip() == "0" and backend(env, "status").get("radio"),
       "fin du mode avion : radios rallumées")
    return work, env


def test_messages():
    module = load("wifi_fr", WIFI)
    ok(module.classify("Error: Not authorized to control networking.") == "not-authorized",
       "« non autorisé » de NetworkManager reconnu")
    ok(module.classify("Error: Connection activation failed: (7) Secrets were required, but not provided.") == "bad-key",
       "clé refusée reconnue")
    ok(module.classify("Error: No network with SSID 'x' found.") == "out-of-range", "réseau introuvable reconnu")
    ok(module.classify("Error: Timeout expired (45 seconds)") == "timeout", "délai dépassé reconnu")
    ok(module.classify("IP configuration could not be reserved (no available address, timeout, etc.).") == "no-ip",
       "pas d'adresse IP reconnu")
    for row in module._TR.values():
        ok(len(row) == 3 and all(row), "traductions : en, es, de") if len(row) != 3 or not all(row) else None
    missing = [code for code, text in module.ERRORS.items() if text not in module._TR]
    ok(not missing, f"chaque erreur est traduite dans les 4 langues ({missing})")
    seen = set()
    for lang in ("fr", "en", "es", "de"):
        localized = load(f"wifi_{lang}", WIFI, lang)
        text = localized.error_text({"error": "bad-key"})
        seen.add(text)
        ok("non autorisé" not in text.lower() and text, f"{lang} : message de clé refusée en clair")
    ok(len(seen) == 4, "message de clé refusée différent dans chaque langue")
    os.environ["WINLUX_LANG"] = "fr"
    root = load("winlux_root_test", HERE / "winlux-root.py")
    level = root.exec_level
    for args, expected in ((["winlux-wifi", "status"], "user"), (["winlux-wifi", "list", "--rescan"], "user"),
                           (["winlux-wifi", "connect", "Box", "--security", "wpa", "--key-stdin"], "user"),
                           (["winlux-wifi", "connect", "Box", "--security", "eap", "--identity", "moi", "--key-stdin"], "user"),
                           (["winlux-wifi", "connect", "Box", "--hidden", "--no-auto", "--security", "sae", "--key-stdin"], "user"),
                           (["winlux-wifi", "airplane", "on"], "user"), (["winlux-wifi", "forget", "Box"], "user"),
                           (["winlux-wifi", "autoconnect", "Box", "off"], "user"),
                           (["winlux-wifi", "connect", "Box", "--security", "rm"], None),
                           (["winlux-wifi", "connect", "--key-stdin"], None), (["winlux-wifi", "radio", "maybe"], None),
                           (["winlux-wifi", "connect", "Box", "--exec", "x"], None), (["winlux-wifi", "sh"], None),
                           (["winlux-wifi", "list", "--rescan", "x"], None),
                           (["winlux-wifi", "connect", "Box", "--security", "eap", "--identity", "moi", "--eap", "peap",
                             "--phase2", "mschapv2", "--ca", "system", "--domain", "radius.exemple.fr", "--key-stdin"], "user"),
                           (["winlux-wifi", "connect", "Box", "--security", "eap", "--identity", "moi", "--eap", "tls",
                             "--ca", "/home/moi/ca.pem", "--client-cert", "/home/moi/alice.p12",
                             "--private-key", "/home/moi/alice.key", "--key-stdin"], "user"),
                           (["winlux-wifi", "connect", "Box", "--security", "eap", "--eap", "md5"], None),
                           (["winlux-wifi", "connect", "Box", "--security", "eap", "--ca", "relatif.pem"], None),
                           (["winlux-wifi", "connect", "Box", "--security", "eap", "--client-cert", "x.p12"], None),
                           (["winlux-wifi", "connect", "Box", "--security", "eap", "--domain", "a;b"], None)):
        ok(level(args) == expected, f"relais root : {' '.join(args)} → {expected or 'refusé'}")


def test_wiring():
    popup = (HERE / "winlux-popup.py").read_text()
    ok('["winlux-shell-app","settings","35"]' in popup and '"winlux-system-tools","quick"' not in popup,
       "tuile Wi-Fi : la flèche ouvre la page Wi-Fi, plus l'outil générique")
    ok('["winlux-wifi","radio"' in popup and '["winlux-wifi","airplane"' in popup,
       "tuiles Wi-Fi et mode avion : par le relais root (plus de « non autorisé »)")
    ok('["winlux-shell-app", "settings", "35"]' in (HERE / "winlux-welcome.py").read_text(),
       "Bienvenue : « Configurer le réseau » ouvre la page Wi-Fi")
    ok('(_t("Réseau Wi-Fi"), 35)' in (HERE / "winlux-start-menu.py").read_text(), "menu Démarrer : « Réseau Wi-Fi » → page Wi-Fi")
    tools = (HERE / "winlux-system-tools.py").read_text()
    ok("nm-connection-editor" not in tools and "Se connecter au réseau sélectionné\"" not in tools.split("_TR = {")[1].split("}\n", 1)[1]
       and 'open_settings("35")' in tools and 'open_settings("22")' in tools,
       "Outils Winlux : Wi-Fi et Bluetooth renvoient vers leurs pages dédiées")
    stage = (HERE / "stage-desktop-components.sh").read_text()
    ok("winlux-root winlux-wifi" in stage and "50-winlux-network.rules" in stage, "image : winlux-wifi et règle polkit livrés")


def pump(Gtk, seconds=0.3):
    end = time.time() + seconds
    while time.time() < end:
        while Gtk.events_pending():
            Gtk.main_iteration_do(False)
        time.sleep(0.02)


def walk(widget, Gtk):
    yield widget
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children():
            yield from walk(child, Gtk)


def test_page(work, env):
    for key in ("PATH", "FAKE_NM_STATE", "WINLUX_WIFI_CONNECTIONS", "WINLUX_SYS_ROOT", "WINLUX_WIFI_DIRECT", "XDG_CONFIG_HOME", "WINLUX_AIRPLANE_MARK", "WINLUX_WIFI_CERTS"):
        os.environ[key] = env[key]
    for path in Path(env["WINLUX_WIFI_CONNECTIONS"]).glob("*"):
        path.unlink()
    (work / "state.json").write_text(json.dumps({"radio": "enabled", "active": ""}))
    os.environ["WINLUX_SETTINGS_PAGE"] = "35"
    os.environ["WINLUX_WIFI_OSK"] = "1"   # tablette sans clavier physique
    os.environ["WINLUX_LANG"] = "fr"
    import runpy
    settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="settings_wifi_test")
    Gtk, GLib = settings["Gtk"], settings["GLib"]
    window = settings["Settings"]()
    ok(window.current == 35 and 35 not in window.hidden_pages, "Paramètres s'ouvre directement sur la page Wi-Fi")
    panel = window.wifi_panel
    deadline = time.time() + 20
    while time.time() < deadline and not panel.known_card.get_children():
        pump(Gtk, 0.1)
    labels = [w.get_text() for w in walk(window.body, Gtk) if isinstance(w, Gtk.Label)]
    ok("Maison:5G" in labels and "Café" in labels, "page : réseaux listés par leur nom")
    ok(any("Sécurisé (WPA2)" in text and "Signal" in text for text in labels), "page : sécurité et force du signal affichées")
    ok(any(text == "Aucun réseau enregistré." for text in labels), "page : réseaux enregistrés (aucun au départ)")
    window.open_page(2)
    buttons = [w.get_label() for w in walk(window.body, Gtk) if isinstance(w, Gtk.Button)]
    ok("Réseaux Wi-Fi…" in buttons, "page Réseau : renvoi vers la page Wi-Fi")
    window.open_page(35)
    panel = window.wifi_panel
    deadline = time.time() + 20
    while time.time() < deadline and not panel.network_box.get_children():
        pump(Gtk, 0.1)
    seen = {}

    def dialog():
        return next((w for w in Gtk.Window.list_toplevels() if isinstance(w, Gtk.Dialog) and w.get_visible()
                     and w.get_role() == "winlux-wifi-key"), None)

    def first_try():
        box = dialog()
        if box is None: return True
        widgets = list(walk(box, Gtk))
        key = next(w for w in widgets if isinstance(w, Gtk.Entry) and w.get_name() == "wifi-key")
        keyboard = next((w for w in widgets if getattr(w, "osk_press_char", None)), None)
        seen["osk"] = keyboard is not None and keyboard.get_mapped()
        if keyboard is not None:
            pump(Gtk, 0.2)
            key_y = key.translate_coordinates(box, 0, 0)[1] + key.get_allocated_height()
            keyboard_y = keyboard.translate_coordinates(box, 0, 0)[1]
            seen["above"] = key_y <= keyboard_y
            for char in "mauvaise-cle":
                keyboard.osk_press_char(char)
            if os.environ.get("WINLUX_WIFI_CAPTURE"):
                pump(Gtk, 0.4)
                from gi.repository import Gdk
                shot = Gdk.pixbuf_get_from_window(box.get_window(), 0, 0, box.get_allocated_width(), box.get_allocated_height())
                shot.savev(os.environ["WINLUX_WIFI_CAPTURE"].replace(".png", "-cle.png"), "png", [], [])
        seen["typed"] = key.get_text()
        seen["hidden"] = not key.get_visibility()
        box.response(Gtk.ResponseType.OK)
        GLib.timeout_add(200, second_try)
        return False

    def second_try():
        box = dialog()
        if box is None: return True
        widgets = list(walk(box, Gtk))
        problem = next(w for w in widgets if isinstance(w, Gtk.Label) and w.get_name() == "wifi-key-error")
        seen["error"] = problem.get_text()
        key = next(w for w in widgets if isinstance(w, Gtk.Entry) and w.get_name() == "wifi-key")
        show = next(w for w in widgets if isinstance(w, Gtk.CheckButton) and w.get_label() == "Afficher les caractères")
        show.set_active(True)
        seen["shown"] = key.get_visibility()
        key.set_text(GOOD_KEY)
        box.response(Gtk.ResponseType.OK)
        return False

    GLib.timeout_add(300, first_try)
    target = next(w for w in walk(panel.network_box, Gtk) if isinstance(w, Gtk.Button)
                  and w.get_accessible().get_name() == "Se connecter Maison:5G")
    target.clicked()
    deadline = time.time() + 30
    while time.time() < deadline and "Connecté à « Maison:5G »." != panel.message.get_text():
        pump(Gtk, 0.1)
    ok(seen.get("osk"), "boîte de la clé : clavier tactile ouvert d'office sans clavier physique")
    ok(seen.get("above"), "boîte de la clé : le champ reste au-dessus du clavier tactile")
    ok(seen.get("typed") == "mauvaise-cle" and seen.get("hidden"), "clavier tactile : la clé s'écrit dans le champ, masquée")
    ok("clé de sécurité est incorrecte" in seen.get("error", ""), f"clé refusée : message clair et la boîte se rouvre ({seen.get('error')})")
    ok(seen.get("shown"), "« Afficher les caractères » montre la clé")
    ok(panel.message.get_text() == "Connecté à « Maison:5G ».", f"connexion réussie annoncée ({panel.message.get_text()})")
    deadline = time.time() + 20
    while time.time() < deadline and not any(isinstance(w, Gtk.Switch) for w in walk(panel.known_card, Gtk)):
        pump(Gtk, 0.1)
    ok(any(isinstance(w, Gtk.Button) and w.get_label() == "Oublier" for w in walk(panel.known_card, Gtk)),
       "réseau mémorisé, avec « Oublier » et « Se connecter automatiquement »")
    ok(any(isinstance(w, Gtk.Button) and w.get_label() == "Ajouter un réseau caché…" for w in walk(window.body, Gtk)),
       "« Ajouter un réseau caché… » proposé")
    if os.environ.get("WINLUX_WIFI_CAPTURE"):
        pump(Gtk, 0.5)
        from gi.repository import Gdk
        pixbuf = Gdk.pixbuf_get_from_window(window.get_window(), 0, 0, window.get_allocated_width(), window.get_allocated_height())
        pixbuf.savev(os.environ["WINLUX_WIFI_CAPTURE"], "png", [], [])
    # Entreprise : méthode, authentification interne, certificat du serveur,
    # domaine ; TLS : certificat personnel au lieu du mot de passe.
    enterprise = {}

    def enterprise_dialog():
        box = dialog()
        if box is None: return True
        widgets = list(walk(box, Gtk))
        combos = [w for w in widgets if isinstance(w, Gtk.ComboBoxText)]
        method = next((c for c in combos if c.get_active_id() == "peap"), None)
        labels = [w.get_text() for w in widgets if isinstance(w, Gtk.Label) and w.get_mapped()]
        enterprise["peap"] = method is not None and all(t in labels for t in (
            "Nom d’utilisateur", "Méthode", "Authentification interne", "Certificat du serveur", "Domaine (facultatif)", "Mot de passe"))
        enterprise["no_cert_row"] = "Certificat personnel" not in labels
        if method is not None:
            method.set_active_id("tls")
            pump(Gtk, 0.2)
            labels = [w.get_text() for w in walk(box, Gtk) if isinstance(w, Gtk.Label) and w.get_mapped()]
            enterprise["tls"] = ("Certificat personnel" in labels and "Authentification interne" not in labels
                                 and "Mot de passe de la clé privée (facultatif)" in labels)
        box.response(Gtk.ResponseType.CANCEL)
        return False

    GLib.timeout_add(300, enterprise_dialog)
    bureau = next(w for w in walk(panel.network_box, Gtk) if isinstance(w, Gtk.Button)
                  and w.get_accessible().get_name() == "Se connecter Bureau")
    bureau.clicked()
    pump(Gtk, 0.5)
    ok(enterprise.get("peap"), "boîte Entreprise : identifiant, méthode, authentification interne, certificat du serveur, domaine")
    ok(enterprise.get("no_cert_row"), "boîte Entreprise PEAP : pas de certificat personnel demandé")
    ok(enterprise.get("tls"), "boîte Entreprise TLS : certificat personnel et mot de passe de la clé privée")
    # Wi-Fi public : bouton « Page de connexion du réseau » sur le réseau ouvert connecté.
    state = json.loads((work / "state.json").read_text())
    state["active"] = "Café"
    (work / "state.json").write_text(json.dumps(state))
    panel.refresh(False)
    deadline = time.time() + 15
    portal = None
    while time.time() < deadline and portal is None:
        pump(Gtk, 0.2)
        portal = next((w for w in walk(panel.network_box, Gtk) if isinstance(w, Gtk.Button) and w.get_name() == "wifi-portal"), None)
    ok(portal is not None and portal.get_label() == "Page de connexion du réseau",
       "Wi-Fi public connecté : « Page de connexion du réseau » proposée")
    window.destroy()


if __name__ == "__main__":
    work, env = test_backend()
    test_messages()
    test_wiring()
    if os.environ.get("DISPLAY"):
        test_page(work, env)
    else:
        print("(page GTK non testée : pas d'écran ; lancer sous xvfb-run)")
    if failures:
        print(f"\n{len(failures)} échec(s)")
        raise SystemExit(1)
    print("\nWi-Fi : tout est bon")
