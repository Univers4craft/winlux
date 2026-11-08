#!/usr/bin/python3
"""Point d'accès sans fil mobile de Winlux : partager la connexion Internet en Wi-Fi.

Deux moitiés dans un seul fichier, installé en /usr/bin/winlux-hotspot :

* le service (sous-commandes ci-dessous), qui parle à NetworkManager. Il
  tourne en root : depuis le bureau, Paramètres et les Réglages rapides passent
  par le relais `winlux-root exec winlux-hotspot …` (pas de polkit utile sans
  logind, #377). Lancé par un compte, ce programme se relaie lui-même.
* la page « Point d'accès sans fil mobile » de Paramètres (classe HotspotPanel).

  winlux-hotspot status                 état, nom, mot de passe, appareils (JSON)
  winlux-hotspot start                  partage la connexion
  winlux-hotspot stop
  winlux-hotspot configure --stdin      JSON : ssid, key, security (wpa2|wpa3),
                                        band (auto|2.4|5) ; mot de passe jamais
                                        dans la ligne de commande

Fonctionnement : un profil NetworkManager « winlux-hotspot » (mode=ap,
ipv4.method=shared : NetworkManager lance dnsmasq pour les adresses et pose la
traduction d'adresses). Chiffrement CCMP (AES) imposé, jamais GCMP-256 seul que
bien des téléphones refusent (#416). La page n'existe que si une carte Wi-Fi
sait faire point d'accès (propriété AP de NetworkManager, sinon « iw list »)
et que dnsmasq est présent.

Une même carte ne peut pas à la fois recevoir Internet en Wi-Fi et le
partager : le service le refuse avec un message clair au lieu de couper la
connexion de l'ordinateur.
"""
import json
import os
from pathlib import Path
import random
import re
import shutil
import string
import subprocess
import sys

SYS_ROOT = Path(os.environ.get("WINLUX_SYS_ROOT", "/"))
CONNECTIONS = Path(os.environ.get("WINLUX_HOTSPOT_CONNECTIONS", "/etc/NetworkManager/system-connections"))
LEASES = Path(os.environ.get("WINLUX_HOTSPOT_LEASES", "/var/lib/NetworkManager"))
RELAY_SOCKET = os.environ.get("WINLUX_ROOT_SOCKET", "/run/winlux-root.sock")
PERMS = os.environ.get("WINLUX_HOTSPOT_PERMS", "/usr/bin/winlux-network-perms")
PROFILE_ID = "winlux-hotspot"
PROFILE_UUID = "6a3df0b6-9cff-4e1a-9a5e-77696e6c7578"
PROFILE_FILE = "winlux-hotspot.nmconnection"
MAX_CLIENTS = 8
START_WAIT = 40


# ── Langue de l'interface : fr, en, es ou de ────────────────────────────────

def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="):
                        code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}

# Texte français -> (anglais, espagnol, allemand).
_TR = {
    "Point d’accès sans fil mobile": ("Mobile hotspot", "Zona con cobertura inalámbrica móvil", "Mobiler Hotspot"),
    "Partagez votre connexion Internet avec d’autres appareils en Wi-Fi.": ("Share your Internet connection with other devices over Wi-Fi.", "Comparta su conexión a Internet con otros dispositivos mediante Wi-Fi.", "Teilen Sie Ihre Internetverbindung per WLAN mit anderen Geräten."),
    "Activé · {count} appareil(s) connecté(s) sur {max}": ("On · {count} of {max} devices connected", "Activado · {count} de {max} dispositivos conectados", "Ein · {count} von {max} Geräten verbunden"),
    "Désactivé": ("Off", "Desactivado", "Aus"),
    "Partager ma connexion Internet depuis": ("Share my Internet connection from", "Compartir mi conexión a Internet desde", "Meine Internetverbindung teilen über"),
    "Aucune connexion à Internet": ("No Internet connection", "Sin conexión a Internet", "Keine Internetverbindung"),
    "Partager en Wi-Fi avec : {device}": ("Share over Wi-Fi with: {device}", "Compartir por Wi-Fi con: {device}", "Über WLAN teilen mit: {device}"),
    "Propriétés": ("Properties", "Propiedades", "Eigenschaften"),
    "Nom": ("Name", "Nombre", "Name"),
    "Mot de passe": ("Password", "Contraseña", "Kennwort"),
    "Bande réseau": ("Network band", "Banda de red", "Netzwerkband"),
    "Sécurité": ("Security", "Seguridad", "Sicherheit"),
    "Toute bande disponible": ("Any available", "Cualquier banda disponible", "Jedes verfügbare Band"),
    "2,4 GHz": ("2.4 GHz", "2,4 GHz", "2,4 GHz"),
    "5 GHz": ("5 GHz", "5 GHz", "5 GHz"),
    "WPA2 Personnel (le plus compatible)": ("WPA2 Personal (most compatible)", "WPA2 Personal (el más compatible)", "WPA2-Personal (am kompatibelsten)"),
    "WPA3 Personnel": ("WPA3 Personal", "WPA3 Personal", "WPA3-Personal"),
    "Modifier": ("Edit", "Editar", "Bearbeiten"),
    "Modifier les informations du réseau": ("Edit network info", "Editar información de red", "Netzwerkinformationen bearbeiten"),
    "Nom du réseau": ("Network name", "Nombre de red", "Netzwerkname"),
    "Mot de passe du réseau (8 caractères minimum)": ("Network password (at least 8 characters)", "Contraseña de red (8 caracteres como mínimo)", "Netzwerkkennwort (mindestens 8 Zeichen)"),
    "Afficher les caractères": ("Show characters", "Mostrar caracteres", "Zeichen anzeigen"),
    "Annuler": ("Cancel", "Cancelar", "Abbrechen"),
    "Enregistrer": ("Save", "Guardar", "Speichern"),
    "Appareils connectés": ("Connected devices", "Dispositivos conectados", "Verbundene Geräte"),
    "Aucun appareil connecté.": ("No devices connected.", "No hay dispositivos conectados.", "Keine Geräte verbunden."),
    "Appareil sans nom": ("Unnamed device", "Dispositivo sin nombre", "Unbenanntes Gerät"),
    "Adresse IP : {ip} · Adresse physique : {mac}": ("IP address: {ip} · Physical address: {mac}", "Dirección IP: {ip} · Dirección física: {mac}", "IP-Adresse: {ip} · Physische Adresse: {mac}"),
    "Démarrage du point d’accès…": ("Starting the hotspot…", "Iniciando la zona con cobertura…", "Hotspot wird gestartet…"),
    "Détail : {detail}": ("Details: {detail}", "Detalle: {detail}", "Details: {detail}"),
    "Le point d’accès n’a pas pu démarrer.": ("The hotspot could not start.", "No se pudo iniciar la zona con cobertura.", "Der Hotspot konnte nicht gestartet werden."),
    "Cette carte Wi-Fi est déjà utilisée pour se connecter à Internet : elle ne peut pas en même temps partager la connexion. Branchez un câble Ethernet (ou un téléphone en USB), puis réessayez.":
        ("This Wi-Fi adapter is already used to connect to the Internet: it cannot share the connection at the same time. Plug in an Ethernet cable (or a phone over USB), then try again.",
         "Este adaptador Wi-Fi ya se usa para conectarse a Internet: no puede compartir la conexión al mismo tiempo. Conecte un cable Ethernet (o un teléfono por USB) y vuelva a intentarlo.",
         "Dieser WLAN-Adapter wird bereits für die Internetverbindung verwendet und kann sie nicht gleichzeitig teilen. Schließen Sie ein Ethernet-Kabel (oder ein Telefon per USB) an und versuchen Sie es erneut."),
    "Cet ordinateur n’a pas de connexion à Internet à partager. Connectez-le d’abord (câble Ethernet, téléphone en USB).":
        ("This computer has no Internet connection to share. Connect it first (Ethernet cable, phone over USB).",
         "Este equipo no tiene ninguna conexión a Internet que compartir. Conéctelo primero (cable Ethernet, teléfono por USB).",
         "Dieser Computer hat keine Internetverbindung zum Teilen. Verbinden Sie ihn zuerst (Ethernet-Kabel, Telefon per USB)."),
    "Aucune carte Wi-Fi de cet ordinateur ne sait créer un point d’accès.": ("No Wi-Fi adapter in this computer can create a hotspot.", "Ningún adaptador Wi-Fi de este equipo puede crear una zona con cobertura.", "Kein WLAN-Adapter dieses Computers kann einen Hotspot erstellen."),
    "Cette carte Wi-Fi refuse ce réglage (WPA3 ou 5 GHz). Choisissez « WPA2 Personnel » et « Toute bande disponible » dans Modifier.":
        ("This Wi-Fi adapter refuses this setting (WPA3 or 5 GHz). Choose “WPA2 Personal” and “Any available” in Edit.",
         "Este adaptador Wi-Fi rechaza esta configuración (WPA3 o 5 GHz). Elija «WPA2 Personal» y «Cualquier banda disponible» en Editar.",
         "Dieser WLAN-Adapter lehnt diese Einstellung ab (WPA3 oder 5 GHz). Wählen Sie unter „Bearbeiten“ „WPA2-Personal“ und „Jedes verfügbare Band“."),
    "Le Wi-Fi est désactivé. Activez-le (et quittez le mode avion), puis réessayez.": ("Wi-Fi is turned off. Turn it on (and leave airplane mode), then try again.", "El Wi-Fi está desactivado. Actívelo (y salga del modo avión) y vuelva a intentarlo.", "WLAN ist ausgeschaltet. Schalten Sie es ein (und beenden Sie den Flugzeugmodus) und versuchen Sie es erneut."),
    "Le nom du réseau compte de 1 à 32 octets.": ("The network name must be 1 to 32 bytes long.", "El nombre de la red debe tener de 1 a 32 bytes.", "Der Netzwerkname muss 1 bis 32 Byte lang sein."),
    "Le mot de passe compte de 8 à 63 caractères, sans accents.": ("The password must be 8 to 63 characters, without accents.", "La contraseña debe tener de 8 a 63 caracteres, sin acentos.", "Das Kennwort muss 8 bis 63 Zeichen lang sein, ohne Akzente."),
    "Le service réseau (NetworkManager) ne répond pas. Redémarrez l’ordinateur.": ("The network service (NetworkManager) is not responding. Restart the computer.", "El servicio de red (NetworkManager) no responde. Reinicie el equipo.", "Der Netzwerkdienst (NetworkManager) antwortet nicht. Starten Sie den Computer neu."),
    "Le service système de Winlux ne répond pas. Redémarrez l’ordinateur.": ("The Winlux system service is not responding. Restart the computer.", "El servicio del sistema de Winlux no responde. Reinicie el equipo.", "Der Winlux-Systemdienst antwortet nicht. Starten Sie den Computer neu."),
    "Ethernet": ("Ethernet", "Ethernet", "Ethernet"),
    "Wi-Fi": ("Wi-Fi", "Wi-Fi", "WLAN"),
    "Réseau mobile": ("Cellular", "Red móvil", "Mobilfunk"),
    "VPN": ("VPN", "VPN", "VPN"),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# ── Service (root) : NetworkManager ─────────────────────────────────────────

def nm(args, timeout=20):
    env = dict(os.environ, LC_ALL="C", LANG="C", LANGUAGE="C")
    try:
        return subprocess.run(["nmcli", *args], capture_output=True, text=True, timeout=timeout, env=env)
    except FileNotFoundError:
        return subprocess.CompletedProcess(args, 127, "", "NetworkManager is not running (nmcli missing)")
    except subprocess.TimeoutExpired:
        return subprocess.CompletedProcess(args, 124, "", "Timeout expired")


def run(args, timeout=5):
    try:
        return subprocess.run(args, capture_output=True, text=True, timeout=timeout,
                              env=dict(os.environ, LC_ALL="C")).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


def split_terse(line):
    fields, field, escaped = [], [], False
    for char in line:
        if escaped:
            field.append(char)
            escaped = False
        elif char == "\\":
            escaped = True
        elif char == ":":
            fields.append("".join(field))
            field = []
        else:
            field.append(char)
    fields.append("".join(field))
    return fields


def classify(message):
    text = (message or "").lower()
    rules = (
        (("not authorized", "insufficient privileges", "permission denied"), "not-authorized"),
        (("is not running", "could not create nmclient", "nmcli missing"), "no-service"),
        (("radio", "wireless is disabled", "wi-fi is disabled", "rfkill"), "wifi-off"),
        (("not supported", "unsupported", "does not support", "incompatible", "sae", "band"), "setting"),
        (("no suitable device", "device not found", "no wi-fi device"), "not-capable"),
    )
    for words, code in rules:
        if any(word in text for word in words):
            return code
    return "failed"


def fail(code, detail="", **values):
    print(json.dumps({"ok": False, "error": code, "detail": (detail or "").strip()[-400:], **values}))
    return 1


def done(**values):
    print(json.dumps({"ok": True, **values}))
    return 0


def hardware_present():
    try:
        return any((SYS_ROOT / "sys/class/ieee80211").glob("*")) or any((SYS_ROOT / "sys/class/net").glob("*/wireless"))
    except OSError:
        return False


def dnsmasq_present():
    """NetworkManager confie adresses et DNS des appareils à dnsmasq (mode shared)."""
    return any((SYS_ROOT / path).exists() for path in ("usr/sbin/dnsmasq", "sbin/dnsmasq", "usr/bin/dnsmasq"))


def wifi_devices():
    """Cartes Wi-Fi gérées par NetworkManager : [(nom, état, connexion)]."""
    found = []
    for line in nm(["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device"], timeout=5).stdout.splitlines():
        fields = split_terse(line)
        if len(fields) >= 4 and fields[1] == "wifi" and "unmanaged" not in fields[2]:
            found.append((fields[0], fields[2], fields[3]))
    return found


def ap_capable(device):
    """La carte sait-elle faire point d'accès ? NetworkManager, sinon iw."""
    answer = nm(["-g", "WIFI-PROPERTIES.AP", "device", "show", device], timeout=5).stdout.strip().lower()
    if answer in ("yes", "no"):
        return answer == "yes"
    if shutil.which("iw"):
        modes = re.search(r"Supported interface modes:(.*?)(?:\n\t\S|\Z)", run(["iw", "list"]), re.DOTALL)
        return bool(modes and re.search(r"\*\s*AP\s*$", modes[1], re.MULTILINE))
    return False


def band_5ghz(device):
    return nm(["-g", "WIFI-PROPERTIES.5GHZ", "device", "show", device], timeout=5).stdout.strip().lower() == "yes"


def capable_devices():
    return [device for device, _, _ in wifi_devices() if ap_capable(device)]


def available():
    """La page n'existe qu'avec une carte capable de point d'accès, NetworkManager et dnsmasq."""
    if not hardware_present() or shutil.which("nmcli") is None or not dnsmasq_present():
        return False
    return bool(capable_devices())


def uplink(exclude=""):
    """Connexion à Internet partagée : (carte, type, nom de la connexion) ou None."""
    best = None
    for line in run(["ip", "-4", "route", "show", "default"]).splitlines():
        fields = line.split()
        if "dev" not in fields:
            continue
        device = fields[fields.index("dev") + 1]
        metric = int(fields[fields.index("metric") + 1]) if "metric" in fields else 0
        if device != exclude and (best is None or metric < best[1]):
            best = (device, metric)
    if best is None:
        return None
    device = best[0]
    kind, name = "", ""
    for line in nm(["-t", "-f", "DEVICE,TYPE,CONNECTION", "device"], timeout=5).stdout.splitlines():
        fields = split_terse(line)
        if len(fields) >= 3 and fields[0] == device:
            kind, name = fields[1], fields[2]
    if not kind:
        kind = "wifi" if (SYS_ROOT / "sys/class/net" / device / "wireless").exists() else "ethernet"
    return {"device": device, "type": kind, "name": name or device}


# ── Profil du point d'accès ─────────────────────────────────────────────────

def keyfile_escape(value):
    value = value.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t")
    return "\\s" + value[1:] if value.startswith(" ") else value


def ssid_bytes(ssid):
    """SSID en liste d'octets : n'importe quel caractère passe tel quel."""
    return "".join(f"{byte};" for byte in ssid.encode("utf-8"))


def ssid_from(value):
    if re.fullmatch(r"([0-9]+;)+", value or ""):
        try: return bytes(int(byte) for byte in value.split(";") if byte).decode("utf-8", "replace")
        except ValueError: return value
    return (value or "").replace("\\s", " ").replace("\\\\", "\\")


def key_from(value):
    return (value or "").replace("\\s", " ", 1) if (value or "").startswith("\\s") else (value or "").replace("\\\\", "\\")


def key_ok(key):
    return 8 <= len(key) <= 63 and all(32 <= ord(char) < 127 for char in key)


def ssid_ok(ssid):
    return 1 <= len(ssid.encode("utf-8")) <= 32 and ssid.isprintable()


def default_settings():
    """Nom et mot de passe proposés la première fois, comme Windows."""
    host = re.sub(r"[^A-Za-z0-9-]", "", (SYS_ROOT / "etc/hostname").read_text().strip()
                  if (SYS_ROOT / "etc/hostname").is_file() else "")[:16] or "WINLUX"
    rng = random.SystemRandom()
    alphabet = string.ascii_letters + string.digits
    alphabet = "".join(char for char in alphabet if char not in "0O1lI")
    key = "".join(rng.choice(alphabet) for _ in range(10))
    return {"ssid": f"{host} {rng.randint(1000, 9999)}", "key": key, "security": "wpa2", "band": "auto"}


def profile_text(settings, device=""):
    lines = ["[connection]", f"id={PROFILE_ID}", f"uuid={PROFILE_UUID}", "type=wifi", "autoconnect=false"]
    if device: lines.append(f"interface-name={device}")
    lines += ["", "[wifi]", "mode=ap", f"ssid={ssid_bytes(settings['ssid'])}"]
    if settings.get("band") == "2.4": lines.append("band=bg")
    if settings.get("band") == "5": lines.append("band=a")
    lines += ["", "[wifi-security]"]
    if settings.get("security") == "wpa3":
        lines += ["key-mgmt=sae", "pmf=3"]
    else:
        # PMF désactivées : de vieux téléphones refusent un point d'accès qui les impose.
        lines += ["key-mgmt=wpa-psk", "pmf=1"]
    # AES (CCMP) seulement : jamais TKIP, jamais GCMP-256 seul (#416).
    lines += ["proto=rsn;", "pairwise=ccmp;", "group=ccmp;", f"psk={keyfile_escape(settings['key'])}", "",
              "[ipv4]", "method=shared", "", "[ipv6]", "addr-gen-mode=default", "method=ignore", ""]
    return "\n".join(lines)


def profile_path():
    return CONNECTIONS / PROFILE_FILE


def read_settings():
    path = profile_path()
    if not path.is_file():
        return None
    values, section = {}, ""
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.startswith("["):
            section = line.strip().strip("[]")
        elif "=" in line:
            key, value = line.split("=", 1)
            values[f"{section}.{key.strip()}"] = value.strip()
    band = {"bg": "2.4", "a": "5"}.get(values.get("wifi.band", ""), "auto")
    return {"ssid": ssid_from(values.get("wifi.ssid", "")), "key": key_from(values.get("wifi-security.psk", "")),
            "security": "wpa3" if values.get("wifi-security.key-mgmt") == "sae" else "wpa2", "band": band,
            "device": values.get("connection.interface-name", "")}


def write_settings(settings, device=""):
    path = profile_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name("." + path.name + ".tmp")
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
        handle.write(profile_text(settings, device))
    os.chmod(temporary, 0o600)
    os.replace(temporary, path)
    result = nm(["connection", "load", str(path)], timeout=10)
    if result.returncode:
        result = nm(["connection", "reload"], timeout=10)
    return result


def ensure_settings():
    settings = read_settings()
    if settings is None:
        settings = default_settings()
        write_settings(settings)
    return settings


def active_device():
    """Carte sur laquelle le point d'accès tourne, '' s'il est arrêté."""
    for line in nm(["-t", "-f", "UUID,DEVICE,STATE", "connection", "show", "--active"], timeout=5).stdout.splitlines():
        fields = split_terse(line)
        if len(fields) >= 3 and fields[0] == PROFILE_UUID:
            return fields[1] or "?"
    return ""


def clients(device):
    """Appareils connectés : stations associées (iw), noms et adresses (baux dnsmasq)."""
    names = {}
    try:
        for line in (LEASES / f"dnsmasq-{device}.leases").read_text().splitlines():
            parts = line.split()
            if len(parts) >= 4:
                names[parts[1].lower()] = (parts[2], "" if parts[3] == "*" else parts[3])
    except OSError:
        pass
    stations = [match.lower() for match in re.findall(r"^Station ([0-9a-fA-F:]{17})", run(["iw", "dev", device, "station", "dump"]), re.MULTILINE)] \
        if shutil.which("iw") else []
    macs = stations if stations or shutil.which("iw") else list(names)
    return [{"mac": mac, "ip": names.get(mac, ("", ""))[0], "name": names.get(mac, ("", ""))[1]} for mac in macs]


def fix_resolv():
    if os.access(PERMS, os.X_OK):
        try: subprocess.run([PERMS], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15)
        except (OSError, subprocess.SubprocessError): pass


def pick_device(link):
    """Carte pour le point d'accès : une carte capable qui ne porte pas Internet."""
    capable = capable_devices()
    if not capable:
        return "", "not-capable"
    free = [device for device in capable if not link or device != link["device"]]
    if not free:
        return "", "same-device"
    return free[0], None


def status():
    running = nm(["-t", "-f", "RUNNING", "general"], timeout=5).stdout.strip() == "running"
    if not running:
        return fail("no-service")
    settings = ensure_settings()
    device = active_device()
    link = uplink(exclude=device)
    capable = capable_devices()
    target = device or next((d for d in capable if not link or d != link["device"]), capable[0] if capable else "")
    return done(active=bool(device), device=device or target, capable=bool(capable),
                five_ghz=bool(target) and band_5ghz(target), ssid=settings["ssid"], key=settings["key"],
                security=settings["security"], band=settings["band"], uplink=link,
                same_device=bool(link and capable and all(d == link["device"] for d in capable)),
                clients=clients(device) if device else [], max_clients=MAX_CLIENTS)


def start():
    settings = ensure_settings()
    link = uplink()
    if link is None:
        return fail("no-internet")
    device, problem = pick_device(link)
    if problem:
        return fail(problem)
    if nm(["-t", "-f", "WIFI", "radio"], timeout=5).stdout.strip() == "disabled":
        return fail("wifi-off")
    result = write_settings(settings, device)
    if result.returncode:
        return fail(classify(result.stderr), result.stderr)
    result = nm(["-w", str(START_WAIT), "connection", "up", "uuid", PROFILE_UUID, "ifname", device], timeout=START_WAIT + 15)
    fix_resolv()
    if result.returncode:
        message = (result.stderr or "") + (result.stdout or "")
        return fail(classify(message), message)
    return done(device=device)


def stop():
    result = nm(["connection", "down", "uuid", PROFILE_UUID], timeout=20)
    fix_resolv()
    message = (result.stderr or "") + (result.stdout or "")
    if result.returncode and "not an active connection" not in message.lower():
        return fail(classify(message), message)
    return done()


def configure(payload):
    current = read_settings() or default_settings()
    settings = dict(current)
    for key in ("ssid", "key", "security", "band"):
        if isinstance(payload.get(key), str):
            settings[key] = payload[key]
    settings["ssid"] = settings["ssid"].strip()
    if not ssid_ok(settings["ssid"]):
        return fail("ssid")
    if not key_ok(settings["key"]):
        return fail("key")
    if settings["security"] not in ("wpa2", "wpa3") or settings["band"] not in ("auto", "2.4", "5"):
        return fail("failed", "security/band")
    device = active_device()
    result = write_settings(settings, current.get("device", "") if device else "")
    if result.returncode:
        return fail(classify(result.stderr), result.stderr)
    if device:
        # Réglages changés pendant le partage : redémarré avec les nouveaux.
        nm(["connection", "down", "uuid", PROFILE_UUID], timeout=20)
        return start()
    return done()


def backend_main(argv):
    if argv == ["status"]: return status()
    if argv == ["start"]: return start()
    if argv == ["stop"]: return stop()
    if argv == ["configure", "--stdin"]:
        try:
            payload = json.loads(sys.stdin.read(8192) or "{}")
        except ValueError:
            return fail("failed")
        return configure(payload if isinstance(payload, dict) else {})
    print(__doc__.strip(), file=sys.stderr)
    return 2


# ── Côté bureau : appel du service, en root par le relais ───────────────────

def relayed():
    return os.environ.get("WINLUX_HOTSPOT_DIRECT") != "1" and os.geteuid() != 0 and os.path.exists(RELAY_SOCKET)


def backend_command(args):
    if not relayed():
        return [sys.executable, "-B", os.path.abspath(__file__), *args]
    return [shutil.which("winlux-root") or "/usr/bin/winlux-root", "exec", "winlux-hotspot", *args]


def call(args, payload=None, timeout=START_WAIT + 60):
    try:
        result = subprocess.run(backend_command(args), input=json.dumps(payload) if payload is not None else "",
                                capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.SubprocessError) as error:
        return {"ok": False, "error": "relay", "detail": str(error)}
    for line in reversed(result.stdout.splitlines()):
        if line.startswith("{"):
            try: return json.loads(line)
            except ValueError: break
    detail = (result.stderr or result.stdout).strip()
    if result.returncode == 125 or "ne répond pas" in detail:
        return {"ok": False, "error": "relay", "detail": detail}
    return {"ok": False, "error": classify(detail) if detail else "failed", "detail": detail}


ERRORS = {
    "same-device": "Cette carte Wi-Fi est déjà utilisée pour se connecter à Internet : elle ne peut pas en même temps partager la connexion. Branchez un câble Ethernet (ou un téléphone en USB), puis réessayez.",
    "no-internet": "Cet ordinateur n’a pas de connexion à Internet à partager. Connectez-le d’abord (câble Ethernet, téléphone en USB).",
    "not-capable": "Aucune carte Wi-Fi de cet ordinateur ne sait créer un point d’accès.",
    "setting": "Cette carte Wi-Fi refuse ce réglage (WPA3 ou 5 GHz). Choisissez « WPA2 Personnel » et « Toute bande disponible » dans Modifier.",
    "wifi-off": "Le Wi-Fi est désactivé. Activez-le (et quittez le mode avion), puis réessayez.",
    "ssid": "Le nom du réseau compte de 1 à 32 octets.",
    "key": "Le mot de passe compte de 8 à 63 caractères, sans accents.",
    "no-service": "Le service réseau (NetworkManager) ne répond pas. Redémarrez l’ordinateur.",
    "relay": "Le service système de Winlux ne répond pas. Redémarrez l’ordinateur.",
    "not-authorized": "Le service système de Winlux ne répond pas. Redémarrez l’ordinateur.",
}


def error_text(answer):
    code = answer.get("error", "failed")
    text = _t(ERRORS.get(code, "Le point d’accès n’a pas pu démarrer."))
    detail = answer.get("detail", "")
    if code not in ERRORS and detail:
        text += "\n" + _t("Détail : {detail}").format(detail=detail.splitlines()[-1][:200])
    return text


def uplink_text(link):
    if not link:
        return _t("Aucune connexion à Internet")
    kind = {"ethernet": "Ethernet", "wifi": "Wi-Fi", "gsm": "Réseau mobile", "cdma": "Réseau mobile",
            "wireguard": "VPN", "vpn": "VPN", "tun": "VPN"}.get(link.get("type"), "")
    name = link.get("name") or link.get("device", "")
    return f"{_t(kind)} ({name})" if kind and name != _t(kind) else name


def page(host):
    return HotspotPanel(host)


class HotspotPanel:
    """Page « Point d'accès sans fil mobile » de Paramètres, construite dans `host`."""

    def __init__(self, host):
        import gi
        gi.require_version("Gtk", "3.0")
        from gi.repository import GLib, Gtk
        self.Gtk, self.GLib = Gtk, GLib
        self.host = host
        self.busy = False
        self.updating = False
        self.state = {}
        box = host.card()
        self.switch = host.switch(box, _t("Point d’accès sans fil mobile"), False, self.toggle,
                                  _t("Partagez votre connexion Internet avec d’autres appareils en Wi-Fi."))
        self.switch.set_name("hotspot-switch")
        self.state_label = self.subtitle_of(self.switch)
        self.uplink_label = Gtk.Label(xalign=1, wrap=True)
        self.uplink_label.set_name("hotspot-uplink")
        host.row(box, _t("Partager ma connexion Internet depuis"), self.uplink_label)
        self.message = Gtk.Label(xalign=0, wrap=True, selectable=True)
        self.message.set_name("hotspot-message")
        self.message.set_no_show_all(True)
        box.pack_start(self.message, False, False, 0)
        host.section(_t("Propriétés"))
        self.properties = host.card()
        self.values = {}
        for key, title in (("ssid", "Nom"), ("key", "Mot de passe"), ("band", "Bande réseau"), ("security", "Sécurité")):
            label = Gtk.Label(xalign=1, selectable=True)
            label.set_name(f"hotspot-{key}")
            host.row(self.properties, _t(title), label)
            self.values[key] = label
        host.actions(self.properties, [(_t("Modifier"), self.edit_dialog)])
        host.section(_t("Appareils connectés"))
        self.devices = host.card()
        GLib.idle_add(lambda: self.refresh() and False)
        GLib.timeout_add_seconds(5, self.poll)

    def subtitle_of(self, widget):
        texts = widget.get_parent().get_children()[0]
        labels = texts.get_children()
        return labels[1] if len(labels) > 1 else labels[0]

    def poll(self):
        if self.switch.get_toplevel() is None or not self.switch.get_realized():
            return self.switch.get_parent() is not None
        if not self.busy: self.refresh()
        return True

    def say(self, text, error=False):
        self.message.set_text(text or "")
        context = self.message.get_style_context()
        (context.add_class if error else context.remove_class)("error")
        self.message.set_visible(bool(text))

    def run(self, args, done, payload=None):
        import threading
        self.busy = True

        def worker():
            answer = call(args, payload)
            self.GLib.idle_add(lambda: (setattr(self, "busy", False), done(answer)) and False)
        threading.Thread(target=worker, daemon=True).start()

    def refresh(self):
        if self.busy: return False
        self.run(["status"], self.got_status)
        return False

    def got_status(self, state):
        if not state.get("ok"):
            self.say(error_text(state), True)
            return
        self.state = state
        self.updating = True
        self.switch.set_active(bool(state.get("active")))
        self.updating = False
        count = len(state.get("clients", []))
        self.state_label.set_text(_t("Activé · {count} appareil(s) connecté(s) sur {max}").format(
            count=count, max=state.get("max_clients", 8)) if state.get("active") else _t("Désactivé"))
        self.uplink_label.set_text(uplink_text(state.get("uplink")))
        self.values["ssid"].set_text(state.get("ssid", ""))
        self.values["key"].set_text(state.get("key", ""))
        self.values["band"].set_text(_t({"2.4": "2,4 GHz", "5": "5 GHz"}.get(state.get("band"), "Toute bande disponible")))
        self.values["security"].set_text(_t("WPA3 Personnel") if state.get("security") == "wpa3" else "WPA2")
        if not state.get("active") and state.get("same_device"):
            self.say(error_text({"error": "same-device"}))
        elif not state.get("active") and not state.get("uplink"):
            self.say(error_text({"error": "no-internet"}))
        elif self.message.get_style_context().has_class("error") is False:
            self.say("")
        self.show_clients(state.get("clients", []))

    def show_clients(self, items):
        for child in self.devices.get_children(): child.destroy()
        if not items:
            self.host.hint(_t("Aucun appareil connecté."), self.devices)
        for item in items:
            self.host.row(self.devices, item.get("name") or _t("Appareil sans nom"), None,
                          _t("Adresse IP : {ip} · Adresse physique : {mac}").format(ip=item.get("ip") or "—", mac=item.get("mac", "")))
        self.devices.show_all()

    def after(self, answer):
        if not answer.get("ok"):
            self.say(error_text(answer), True)
        else:
            self.say("")
        self.refresh()

    def toggle(self, on):
        if self.updating: return
        if on:
            self.say(_t("Démarrage du point d’accès…"))
            self.run(["start"], self.after)
        else:
            self.run(["stop"], self.after)

    def edit_dialog(self):
        Gtk = self.Gtk
        dialog = Gtk.Dialog(title=_t("Modifier les informations du réseau"), transient_for=self.host, modal=True)
        dialog.set_role("winlux-hotspot")
        dialog.add_button(_t("Annuler"), Gtk.ResponseType.CANCEL)
        save = dialog.add_button(_t("Enregistrer"), Gtk.ResponseType.OK)
        save.get_style_context().add_class("suggested-action")
        dialog.set_default_response(Gtk.ResponseType.OK)
        area = dialog.get_content_area()
        area.set_spacing(8)
        area.set_border_width(16)

        def entry(title, text, secret=False):
            area.pack_start(Gtk.Label(label=title, xalign=0), False, False, 0)
            widget = Gtk.Entry(text=text, visibility=not secret, activates_default=True)
            area.pack_start(widget, False, False, 0)
            return widget
        ssid = entry(_t("Nom du réseau"), self.state.get("ssid", ""))
        ssid.set_name("hotspot-edit-ssid")
        key = entry(_t("Mot de passe du réseau (8 caractères minimum)"), self.state.get("key", ""), secret=True)
        key.set_name("hotspot-edit-key")
        show = Gtk.CheckButton(label=_t("Afficher les caractères"))
        show.connect("toggled", lambda button: key.set_visibility(button.get_active()))
        area.pack_start(show, False, False, 0)
        area.pack_start(Gtk.Label(label=_t("Bande réseau"), xalign=0), False, False, 0)
        band = Gtk.ComboBoxText()
        band.append("auto", _t("Toute bande disponible"))
        band.append("2.4", _t("2,4 GHz"))
        if self.state.get("five_ghz"): band.append("5", _t("5 GHz"))
        band.set_active_id(self.state.get("band", "auto"))
        if band.get_active_id() is None: band.set_active_id("auto")
        area.pack_start(band, False, False, 0)
        area.pack_start(Gtk.Label(label=_t("Sécurité"), xalign=0), False, False, 0)
        security = Gtk.ComboBoxText()
        security.append("wpa2", _t("WPA2 Personnel (le plus compatible)"))
        security.append("wpa3", _t("WPA3 Personnel"))
        security.set_active_id(self.state.get("security", "wpa2"))
        area.pack_start(security, False, False, 0)
        problem = Gtk.Label(xalign=0, wrap=True)
        problem.get_style_context().add_class("error")
        problem.set_no_show_all(True)
        area.pack_start(problem, False, False, 0)
        dialog.show_all()
        while dialog.run() == Gtk.ResponseType.OK:
            if not ssid_ok(ssid.get_text().strip()):
                problem.set_text(_t(ERRORS["ssid"])); problem.show(); continue
            if not key_ok(key.get_text()):
                problem.set_text(_t(ERRORS["key"])); problem.show(); continue
            payload = {"ssid": ssid.get_text().strip(), "key": key.get_text(),
                       "band": band.get_active_id() or "auto", "security": security.get_active_id() or "wpa2"}
            self.run(["configure", "--stdin"], self.after, payload)
            break
        dialog.destroy()


def main(argv):
    if argv and argv[0] != "--help" and relayed():
        tool = shutil.which("winlux-root") or "/usr/bin/winlux-root"
        os.execv(tool, [tool, "exec", "winlux-hotspot", *argv])
    return backend_main(argv)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
