#!/usr/bin/python3
"""Wi-Fi de Winlux : réseaux, clé de sécurité, WPS, réseaux cachés, mode avion.

Deux moitiés dans un seul fichier, installé en /usr/bin/winlux-wifi :

* le service (sous-commandes ci-dessous), qui parle à NetworkManager. Il
  tourne en root : depuis le bureau d'un compte, Paramètres passe par le
  relais `winlux-root exec winlux-wifi …`. Sans systemd ni logind, polkit ne
  voit jamais la session « active » et NetworkManager répondait « non
  autorisé » à nmcli lancé par l'utilisateur : c'était le blocage du Wi-Fi.
* la page Wi-Fi de Paramètres (classe WifiPanel), chargée par
  winlux-settings-pages comme les autres pages livrées à part.

  winlux-wifi status                    état (JSON)
  winlux-wifi list [--rescan]           réseaux visibles (JSON)
  winlux-wifi known                     réseaux enregistrés (JSON)
  winlux-wifi connect SSID [--security wpa|sae|wep|owe|eap|open]
                     [--hidden] [--no-auto] [--identity NOM] [--key-stdin]
                                        clé lue sur l'entrée standard, jamais
                                        dans la ligne de commande (visible de tous)
  winlux-wifi up SSID                   réseau déjà enregistré
  winlux-wifi wps [SSID]                bouton WPS de la box (2 minutes)
  winlux-wifi forget SSID               oublie le réseau (profil et clé)
  winlux-wifi autoconnect SSID on|off   reconnexion automatique au démarrage
  winlux-wifi disconnect
  winlux-wifi radio on|off              Wi-Fi activé ou non
  winlux-wifi airplane on|off           mode avion (Wi-Fi, Bluetooth, mobile)

Chaque réseau rejoint devient un profil système de NetworkManager
(/etc/NetworkManager/system-connections, 0600, autoconnect) : il se
reconnecte seul au démarrage, pour tous les comptes, avant l'ouverture de
session. Les erreurs sont rendues sous forme de codes (bad-key,
out-of-range…) que la page affiche en clair dans les quatre langues.
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import uuid

SYS_ROOT = Path(os.environ.get("WINLUX_SYS_ROOT", "/"))
CONNECTIONS = Path(os.environ.get("WINLUX_WIFI_CONNECTIONS", "/etc/NetworkManager/system-connections"))
RELAY_SOCKET = os.environ.get("WINLUX_ROOT_SOCKET", "/run/winlux-root.sock")
AIRPLANE = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "winlux/airplane-mode"
SECURITIES = ("open", "wpa", "sae", "wep", "owe", "eap")
CONNECT_WAIT = 45
WPS_WAIT = 120


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
    "Wi-Fi": ("Wi-Fi", "Wi-Fi", "WLAN"),
    "Mode avion": ("Airplane mode", "Modo avión", "Flugzeugmodus"),
    "Coupe le Wi-Fi, le Bluetooth et le réseau mobile.": ("Turns off Wi-Fi, Bluetooth and mobile data.", "Desactiva el Wi-Fi, el Bluetooth y los datos móviles.", "Schaltet WLAN, Bluetooth und Mobilfunk aus."),
    "Connecté à « {name} »": ("Connected to “{name}”", "Conectado a «{name}»", "Verbunden mit „{name}“"),
    "Non connecté": ("Not connected", "Sin conexión", "Nicht verbunden"),
    "Désactivé": ("Off", "Desactivado", "Aus"),
    "Réseaux disponibles": ("Available networks", "Redes disponibles", "Verfügbare Netzwerke"),
    "Cliquez sur un réseau pour vous y connecter. Winlux le retient et s’y reconnecte seul au démarrage.": ("Click a network to connect. Winlux remembers it and reconnects automatically at startup.", "Haga clic en una red para conectarse. Winlux la recuerda y se reconecta solo al iniciar.", "Klicken Sie auf ein Netzwerk, um sich zu verbinden. Winlux merkt es sich und verbindet sich beim Start automatisch."),
    "Recherche des réseaux…": ("Looking for networks…", "Buscando redes…", "Netzwerke werden gesucht…"),
    "Aucun réseau Wi-Fi à portée.": ("No Wi-Fi network in range.", "No hay ninguna red Wi-Fi al alcance.", "Kein WLAN in Reichweite."),
    "Activez le Wi-Fi pour voir les réseaux à proximité.": ("Turn on Wi-Fi to see nearby networks.", "Active el Wi-Fi para ver las redes cercanas.", "Schalten Sie WLAN ein, um Netzwerke in der Nähe zu sehen."),
    "Actualiser": ("Refresh", "Actualizar", "Aktualisieren"),
    "Ajouter un réseau caché…": ("Add a hidden network…", "Agregar una red oculta…", "Verborgenes Netzwerk hinzufügen…"),
    "Se connecter": ("Connect", "Conectar", "Verbinden"),
    "Déconnecter": ("Disconnect", "Desconectar", "Trennen"),
    "Connecté": ("Connected", "Conectado", "Verbunden"),
    "Sécurisé": ("Secured", "Protegida", "Gesichert"),
    "Ouvert": ("Open", "Abierta", "Offen"),
    "Entreprise": ("Enterprise", "Empresa", "Unternehmen"),
    "Enregistré": ("Saved", "Guardada", "Gespeichert"),
    "WPS": ("WPS", "WPS", "WPS"),
    "Signal : {level}": ("Signal: {level}", "Señal: {level}", "Signal: {level}"),
    "excellent": ("excellent", "excelente", "ausgezeichnet"),
    "bon": ("good", "buena", "gut"),
    "moyen": ("fair", "media", "mittel"),
    "faible": ("weak", "débil", "schwach"),
    "Réseaux enregistrés": ("Saved networks", "Redes guardadas", "Gespeicherte Netzwerke"),
    "Aucun réseau enregistré.": ("No saved network.", "Ninguna red guardada.", "Kein gespeichertes Netzwerk."),
    "Se connecter automatiquement": ("Connect automatically", "Conectar automáticamente", "Automatisch verbinden"),
    "Oublier": ("Forget", "Olvidar", "Entfernen"),
    "Oublier « {name} » ?": ("Forget “{name}”?", "¿Olvidar «{name}»?", "„{name}“ entfernen?"),
    "La clé de sécurité sera effacée de cet ordinateur.": ("The security key will be erased from this computer.", "La clave de seguridad se borrará de este equipo.", "Der Sicherheitsschlüssel wird von diesem Computer gelöscht."),
    "Annuler": ("Cancel", "Cancelar", "Abbrechen"),
    "Suivant": ("Next", "Siguiente", "Weiter"),
    "Connexion à « {name} »": ("Connect to “{name}”", "Conectarse a «{name}»", "Verbindung mit „{name}“"),
    "Entrez la clé de sécurité du réseau": ("Enter the network security key", "Escriba la clave de seguridad de red", "Geben Sie den Netzwerksicherheitsschlüssel ein"),
    "Clé de sécurité": ("Security key", "Clave de seguridad", "Sicherheitsschlüssel"),
    "Nom d’utilisateur": ("User name", "Nombre de usuario", "Benutzername"),
    "Mot de passe": ("Password", "Contraseña", "Kennwort"),
    "Afficher les caractères": ("Show characters", "Mostrar caracteres", "Zeichen anzeigen"),
    "Clavier tactile": ("Touch keyboard", "Teclado táctil", "Bildschirmtastatur"),
    "Vous pouvez aussi vous connecter en appuyant sur le bouton WPS de votre box.": ("You can also connect by pressing the WPS button on your router.", "También puede conectarse pulsando el botón WPS del router.", "Sie können sich auch verbinden, indem Sie die WPS-Taste am Router drücken."),
    "Utiliser le bouton WPS": ("Use the WPS button", "Usar el botón WPS", "WPS-Taste verwenden"),
    "Appuyez maintenant sur le bouton WPS de votre box (vous avez 2 minutes)…": ("Press the WPS button on your router now (you have 2 minutes)…", "Pulse ahora el botón WPS del router (tiene 2 minutos)…", "Drücken Sie jetzt die WPS-Taste am Router (Sie haben 2 Minuten)…"),
    "Connexion à « {name} »…": ("Connecting to “{name}”…", "Conectando a «{name}»…", "Verbindung mit „{name}“ wird hergestellt…"),
    "Connecté à « {name} ».": ("Connected to “{name}”.", "Conectado a «{name}».", "Mit „{name}“ verbunden."),
    "Réseau caché": ("Hidden network", "Red oculta", "Verborgenes Netzwerk"),
    "Nom du réseau (SSID)": ("Network name (SSID)", "Nombre de red (SSID)", "Netzwerkname (SSID)"),
    "Type de sécurité": ("Security type", "Tipo de seguridad", "Sicherheitstyp"),
    "WPA2/WPA3 Personnel (le plus courant)": ("WPA2/WPA3-Personal (most common)", "WPA2/WPA3 Personal (el más habitual)", "WPA2/WPA3-Personal (am häufigsten)"),
    "WPA3 Personnel uniquement": ("WPA3-Personal only", "Solo WPA3 Personal", "Nur WPA3-Personal"),
    "WPA2/WPA3 Entreprise": ("WPA2/WPA3-Enterprise", "WPA2/WPA3 Empresa", "WPA2/WPA3-Enterprise"),
    "WEP (ancien, peu sûr)": ("WEP (old, insecure)", "WEP (antiguo, poco seguro)", "WEP (veraltet, unsicher)"),
    "Aucune (réseau ouvert)": ("None (open network)", "Ninguna (red abierta)", "Keine (offenes Netzwerk)"),
    "La connexion Wi-Fi a échoué": ("The Wi-Fi connection failed", "La conexión Wi-Fi ha fallado", "Die WLAN-Verbindung ist fehlgeschlagen"),
    "La clé de sécurité est incorrecte. Vérifiez-la (majuscules comprises) et réessayez.": ("The security key is incorrect. Check it (including capital letters) and try again.", "La clave de seguridad no es correcta. Revísela (incluidas las mayúsculas) e inténtelo de nuevo.", "Der Sicherheitsschlüssel ist falsch. Prüfen Sie ihn (auch Groß- und Kleinschreibung) und versuchen Sie es erneut."),
    "Une clé WPA compte de 8 à 63 caractères (ou 64 chiffres hexadécimaux).": ("A WPA key has 8 to 63 characters (or 64 hexadecimal digits).", "Una clave WPA tiene de 8 a 63 caracteres (o 64 dígitos hexadecimales).", "Ein WPA-Schlüssel hat 8 bis 63 Zeichen (oder 64 Hexadezimalziffern)."),
    "Une clé WEP compte 5 ou 13 caractères (ou 10 ou 26 chiffres hexadécimaux).": ("A WEP key has 5 or 13 characters (or 10 or 26 hexadecimal digits).", "Una clave WEP tiene 5 o 13 caracteres (o 10 o 26 dígitos hexadecimales).", "Ein WEP-Schlüssel hat 5 oder 13 Zeichen (oder 10 oder 26 Hexadezimalziffern)."),
    "Entrez votre nom d’utilisateur et votre mot de passe.": ("Enter your user name and password.", "Escriba su nombre de usuario y su contraseña.", "Geben Sie Benutzername und Kennwort ein."),
    "Le réseau « {name} » est hors de portée. Rapprochez-vous de la box ou vérifiez le nom du réseau.": ("The network “{name}” is out of range. Move closer to the router or check the network name.", "La red «{name}» está fuera de alcance. Acérquese al router o compruebe el nombre de la red.", "Das Netzwerk „{name}“ ist außer Reichweite. Gehen Sie näher an den Router oder prüfen Sie den Netzwerknamen."),
    "La box n’a pas répondu à temps. Réessayez ; si cela se reproduit, redémarrez la box.": ("The router did not answer in time. Try again; if it happens again, restart the router.", "El router no respondió a tiempo. Vuelva a intentarlo; si se repite, reinicie el router.", "Der Router hat nicht rechtzeitig geantwortet. Versuchen Sie es erneut; wenn es wieder passiert, starten Sie den Router neu."),
    "Le Wi-Fi est relié, mais la box n’a pas donné d’adresse à cet ordinateur. Redémarrez la box ou réessayez.": ("Wi-Fi is linked, but the router did not give this computer an address. Restart the router or try again.", "El Wi-Fi está enlazado, pero el router no dio ninguna dirección a este equipo. Reinicie el router o inténtelo de nuevo.", "Das WLAN ist verbunden, aber der Router hat diesem Computer keine Adresse gegeben. Starten Sie den Router neu oder versuchen Sie es erneut."),
    "Le Wi-Fi est désactivé. Activez-le (et quittez le mode avion), puis réessayez.": ("Wi-Fi is off. Turn it on (and leave airplane mode), then try again.", "El Wi-Fi está desactivado. Actívelo (y salga del modo avión) y vuelva a intentarlo.", "WLAN ist aus. Schalten Sie es ein (und beenden Sie den Flugzeugmodus) und versuchen Sie es erneut."),
    "Le service réseau (NetworkManager) ne répond pas. Redémarrez l’ordinateur.": ("The network service (NetworkManager) is not responding. Restart the computer.", "El servicio de red (NetworkManager) no responde. Reinicie el equipo.", "Der Netzwerkdienst (NetworkManager) antwortet nicht. Starten Sie den Computer neu."),
    "Aucune carte Wi-Fi utilisable n’a été trouvée.": ("No usable Wi-Fi adapter was found.", "No se encontró ninguna tarjeta Wi-Fi utilizable.", "Es wurde kein verwendbarer WLAN-Adapter gefunden."),
    "Aucune box n’a répondu au WPS en 2 minutes. Appuyez sur le bouton WPS de la box, puis réessayez.": ("No router answered WPS within 2 minutes. Press the router's WPS button, then try again.", "Ningún router respondió al WPS en 2 minutos. Pulse el botón WPS del router y vuelva a intentarlo.", "Kein Router hat innerhalb von 2 Minuten auf WPS geantwortet. Drücken Sie die WPS-Taste am Router und versuchen Sie es erneut."),
    "Plusieurs box sont en mode WPS en même temps. Attendez deux minutes et réessayez.": ("Several routers are in WPS mode at the same time. Wait two minutes and try again.", "Varios routers están en modo WPS a la vez. Espere dos minutos y vuelva a intentarlo.", "Mehrere Router sind gleichzeitig im WPS-Modus. Warten Sie zwei Minuten und versuchen Sie es erneut."),
    "Cette carte Wi-Fi ne gère pas le WPS : entrez la clé de sécurité.": ("This Wi-Fi adapter does not support WPS: enter the security key.", "Esta tarjeta Wi-Fi no admite WPS: escriba la clave de seguridad.", "Dieser WLAN-Adapter unterstützt kein WPS: Geben Sie den Sicherheitsschlüssel ein."),
    "Le service système de Winlux ne répond pas. Redémarrez l’ordinateur.": ("The Winlux system service is not responding. Restart the computer.", "El servicio del sistema de Winlux no responde. Reinicie el equipo.", "Der Winlux-Systemdienst antwortet nicht. Starten Sie den Computer neu."),
    "La connexion a échoué.": ("The connection failed.", "La conexión ha fallado.", "Die Verbindung ist fehlgeschlagen."),
    "L’opération a échoué.": ("The operation failed.", "La operación ha fallado.", "Der Vorgang ist fehlgeschlagen."),
    "Détail : {detail}": ("Details: {detail}", "Detalle: {detail}", "Details: {detail}"),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# ── Service (root) : NetworkManager ─────────────────────────────────────────

def nm(args, timeout=20, check=False):
    """nmcli en anglais (messages analysés), sans jamais rester bloqué."""
    env = dict(os.environ, LC_ALL="C", LANG="C", LANGUAGE="C")
    try:
        result = subprocess.run(["nmcli", *args], capture_output=True, text=True, timeout=timeout, env=env)
    except FileNotFoundError:
        return subprocess.CompletedProcess(args, 127, "", "NetworkManager is not running (nmcli missing)")
    except subprocess.TimeoutExpired:
        return subprocess.CompletedProcess(args, 124, "", "Timeout expired")
    if check and result.returncode:
        raise SystemExit(fail(classify(result.stderr or result.stdout), result.stderr or result.stdout))
    return result


def split_terse(line):
    """Champs d'une ligne « nmcli -t » (deux-points et barres obliques échappés)."""
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


def security_kind(text):
    """Champ SECURITY de nmcli -> open, wpa, sae, wep, owe ou eap."""
    words = (text or "").replace("--", "").upper().split()
    if not words: return "open"
    if "802.1X" in words: return "eap"
    if "OWE" in words and len(words) == 1: return "owe"
    if words == ["WPA3"]: return "sae"
    if any(word.startswith("WPA") for word in words): return "wpa"
    if "WEP" in words: return "wep"
    return "wpa"


def classify(message):
    """Message d'erreur de nmcli -> code compris par la page."""
    text = (message or "").lower()
    rules = (
        (("not authorized", "insufficient privileges", "permission denied"), "not-authorized"),
        (("is not running", "could not create nmclient", "nmcli missing"), "no-service"),
        (("secrets were required", "no secrets", "no-secrets", "supplicant-disconnect", "wrong password",
          "802.1x supplicant", "4-way handshake", "invalid psk",
          "property is invalid: 802-11-wireless-security.psk"), "bad-key"),
        (("no network with ssid", "could not be found", "not found", "ssid-not-found", "no network"), "out-of-range"),
        (("ip configuration", "ip-config", "dhcp"), "no-ip"),
        (("radio", "wireless is disabled", "wi-fi is disabled", "rfkill"), "wifi-off"),
        (("no wi-fi device", "no suitable device", "no wifi device", "device not found"), "no-device"),
        (("timeout", "timed out"), "timeout"),
    )
    for words, code in rules:
        if any(word in text for word in words):
            return code
    return "failed"


def fail(code, detail=""):
    print(json.dumps({"ok": False, "error": code, "detail": (detail or "").strip()[-400:]}))
    return 1


def done(**values):
    print(json.dumps({"ok": True, **values}))
    return 0


def sys_present(directory, pattern="*"):
    try:
        return any((SYS_ROOT / directory).glob(pattern))
    except OSError:
        return False


def hardware_present():
    return sys_present("sys/class/ieee80211") or sys_present("sys/class/net", "*/wireless")


def wifi_device():
    result = nm(["-t", "-f", "DEVICE,TYPE,STATE", "device"], timeout=5)
    for line in result.stdout.splitlines():
        fields = split_terse(line)
        if len(fields) >= 3 and fields[1] == "wifi" and "unavailable" not in fields[2] and "unmanaged" not in fields[2]:
            return fields[0]
    for line in result.stdout.splitlines():
        fields = split_terse(line)
        if len(fields) >= 2 and fields[1] == "wifi":
            return fields[0]
    return ""


def radio_on():
    return nm(["-t", "-f", "WIFI", "radio"], timeout=5).stdout.strip() == "enabled"


def rfkill_entries(kind):
    """Commutateurs radio du noyau (/sys/class/rfkill) d'un type : wlan, bluetooth, wwan."""
    entries = []
    try:
        for entry in sorted((SYS_ROOT / "sys/class/rfkill").glob("rfkill*")):
            try:
                if (entry / "type").read_text().strip() == kind:
                    entries.append(entry)
            except OSError:
                continue
    except OSError:
        pass
    return entries


def set_rfkill(kind, blocked):
    for entry in rfkill_entries(kind):
        try:
            (entry / "soft").write_text("1\n" if blocked else "0\n")
        except OSError:
            pass


def airplane_state():
    """Mode avion : Wi-Fi coupé et Bluetooth coupé (ou absent)."""
    if radio_on(): return False
    bluetooth = rfkill_entries("bluetooth")
    try:
        return all((entry / "soft").read_text().strip() == "1" for entry in bluetooth)
    except OSError:
        return False


def scan(rescan=False):
    args = ["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY,BSSID", "device", "wifi", "list"]
    if rescan: args += ["--rescan", "yes"]
    result = nm(args, timeout=25)
    networks = {}
    for line in result.stdout.splitlines():
        fields = split_terse(line)
        if len(fields) < 4 or not fields[1]:
            continue
        in_use, ssid, signal, security = fields[:4]
        try: level = int(signal)
        except ValueError: level = 0
        # Une box à plusieurs bornes (ou 2,4 et 5 GHz) : une seule ligne, le meilleur signal.
        current = networks.get(ssid)
        if current is None:
            networks[ssid] = {"ssid": ssid, "signal": level, "security": security_kind(security),
                              "security_text": security.replace("--", "").strip(), "in_use": False}
            current = networks[ssid]
        current["signal"] = max(current["signal"], level)
        current["in_use"] = current["in_use"] or in_use.strip() == "*"
    return sorted(networks.values(), key=lambda n: (not n["in_use"], -n["signal"], n["ssid"].lower()))


def profiles():
    """Profils Wi-Fi enregistrés : liste de dict (uuid, ssid, autoconnect, active)."""
    result = nm(["-t", "-f", "UUID,TYPE,AUTOCONNECT,ACTIVE,NAME", "connection", "show"], timeout=8)
    found = []
    for line in result.stdout.splitlines():
        fields = split_terse(line)
        if len(fields) < 5 or fields[1] not in ("802-11-wireless", "wifi"):
            continue
        ssid_result = nm(["-g", "802-11-wireless.ssid", "connection", "show", "uuid", fields[0]], timeout=5)
        ssid = split_terse(ssid_result.stdout.rstrip("\n"))[0] if ssid_result.returncode == 0 else ""
        found.append({"uuid": fields[0], "ssid": ssid or fields[4], "name": fields[4],
                      "autoconnect": fields[2] == "yes", "active": fields[3] == "yes"})
    return found


def profiles_for(ssid):
    return [p for p in profiles() if p["ssid"] == ssid]


def wps_bsses(device):
    """SSID des box qui annoncent le WPS (wpa_supplicant, D-Bus système), None sans WPS."""
    try:
        from gi.repository import Gio, GLib
    except ImportError:
        return None
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SYSTEM, None)
        supplicant = Gio.DBusProxy.new_sync(bus, 0, None, "fi.w1.wpa_supplicant1", "/fi/w1/wpa_supplicant1",
                                            "fi.w1.wpa_supplicant1", None)
        path = supplicant.call_sync("GetInterface", GLib.Variant("(s)", (device,)), 0, 3000, None).unpack()[0]
        properties = Gio.DBusProxy.new_sync(bus, 0, None, "fi.w1.wpa_supplicant1", path,
                                            "org.freedesktop.DBus.Properties", None)
        names = set()
        bsses = properties.call_sync("Get", GLib.Variant("(ss)", ("fi.w1.wpa_supplicant1.Interface", "BSSs")),
                                     0, 3000, None).unpack()[0]
        for bss in bsses:
            bss_properties = Gio.DBusProxy.new_sync(bus, 0, None, "fi.w1.wpa_supplicant1", bss,
                                                    "org.freedesktop.DBus.Properties", None)
            values = bss_properties.call_sync("GetAll", GLib.Variant("(s)", ("fi.w1.wpa_supplicant1.BSS",)),
                                              0, 3000, None).unpack()[0]
            if (values.get("WPS") or {}).get("Type") in ("pbc", "pin"):
                names.add(bytes(values.get("SSID") or b"").decode("utf-8", errors="replace"))
        return names
    except Exception:
        return None


# ── Profils NetworkManager (fichiers keyfile, jamais de clé en argument) ────

def keyfile_escape(value):
    """Valeur GKeyFile : barre oblique, retours à la ligne, espace initial."""
    value = value.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t")
    return "\\s" + value[1:] if value.startswith(" ") else value


def keyfile_ssid(ssid):
    raw = ssid.encode("utf-8")
    if ssid.isprintable() and ";" not in ssid and ssid == ssid.strip() and "\\" not in ssid:
        return ssid
    # Forme liste d'octets, que NetworkManager relit telle quelle.
    return "".join(f"{byte};" for byte in raw)


def key_problem(security, key, identity=""):
    """None si la clé convient au type de sécurité, sinon le code d'erreur."""
    if security in ("open", "owe"): return None
    if security == "eap": return None if identity and key else "eap-identity"
    if security == "wep":
        if len(key) in (5, 13) and key.isascii() and key.isprintable(): return None
        if len(key) in (10, 26) and re.fullmatch(r"[0-9A-Fa-f]+", key): return None
        return "wep-format"
    if len(key) == 64 and re.fullmatch(r"[0-9A-Fa-f]+", key): return None
    if 8 <= len(key) <= 63 and key.isascii() and all(32 <= ord(c) < 127 for c in key): return None
    return "key-format"


def profile_text(ssid, security, key="", identity="", hidden=False, autoconnect=True, ident=None):
    ident = ident or str(uuid.uuid4())
    lines = ["[connection]", f"id={keyfile_escape(ssid)}", f"uuid={ident}", "type=wifi",
             f"autoconnect={'true' if autoconnect else 'false'}", "",
             "[wifi]", "mode=infrastructure", f"ssid={keyfile_ssid(ssid)}"]
    if hidden: lines.append("hidden=true")
    lines.append("")
    if security == "wpa":
        lines += ["[wifi-security]", "key-mgmt=wpa-psk", f"psk={keyfile_escape(key)}", ""]
    elif security == "sae":
        lines += ["[wifi-security]", "key-mgmt=sae", f"psk={keyfile_escape(key)}", ""]
    elif security == "owe":
        lines += ["[wifi-security]", "key-mgmt=owe", ""]
    elif security == "wep":
        lines += ["[wifi-security]", "key-mgmt=none", "auth-alg=open", "wep-key-type=1",
                  "wep-tx-keyidx=0", f"wep-key0={keyfile_escape(key)}", ""]
    elif security == "eap":
        lines += ["[wifi-security]", "key-mgmt=wpa-eap", "",
                  "[802-1x]", "eap=peap;", f"identity={keyfile_escape(identity)}",
                  f"password={keyfile_escape(key)}", "phase2-auth=mschapv2", ""]
    lines += ["[ipv4]", "method=auto", "", "[ipv6]", "addr-gen-mode=default", "method=auto", ""]
    return "\n".join(lines), ident


def write_profile(text, ident):
    CONNECTIONS.mkdir(parents=True, exist_ok=True)
    path = CONNECTIONS / f"winlux-{ident}.nmconnection"
    temporary = path.with_suffix(".tmp")
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
        handle.write(text)
    os.chmod(temporary, 0o600)
    os.replace(temporary, path)
    return path


def activate(ident, ssid):
    """Active un profil ; code d'erreur ou None."""
    device = wifi_device()
    if not device:
        return "no-device", ""
    if not radio_on():
        return "wifi-off", ""
    result = nm(["-w", str(CONNECT_WAIT), "connection", "up", "uuid", ident, "ifname", device], timeout=CONNECT_WAIT + 15)
    if result.returncode == 0:
        return None, ""
    message = (result.stderr or "") + (result.stdout or "")
    return classify(message), message


def remove_profile(ident, path=None):
    nm(["connection", "delete", "uuid", ident], timeout=10)
    if path is not None:
        try: path.unlink()
        except OSError: pass


def connect(ssid, security, key="", identity="", hidden=False, autoconnect=True):
    if not ssid or len(ssid.encode("utf-8")) > 32:
        return fail("bad-ssid")
    if security not in SECURITIES:
        return fail("failed", "security")
    problem = key_problem(security, key, identity)
    if problem:
        return fail(problem)
    previous = profiles_for(ssid)
    text, ident = profile_text(ssid, security, key, identity, hidden, autoconnect)
    try:
        path = write_profile(text, ident)
    except OSError as error:
        return fail("not-authorized", str(error))
    loaded = nm(["connection", "load", str(path)], timeout=10)
    if loaded.returncode:
        reload = nm(["connection", "reload"], timeout=10)
        if reload.returncode:
            path.unlink(missing_ok=True)
            return fail(classify(reload.stderr), reload.stderr)
    error, detail = activate(ident, ssid)
    if error:
        # Une clé refusée n'est pas gardée ; l'ancien profil (s'il existait) reste.
        remove_profile(ident, path)
        return fail(error, detail)
    for old in previous:
        if old["uuid"] != ident:
            nm(["connection", "delete", "uuid", old["uuid"]], timeout=10)
    return done(ssid=ssid)


def up(ssid):
    known = profiles_for(ssid)
    if not known:
        return fail("unknown")
    error, detail = activate(known[0]["uuid"], ssid)
    return fail(error, detail) if error else done(ssid=ssid)


def forget(ssid):
    known = profiles_for(ssid)
    for profile in known:
        nm(["connection", "delete", "uuid", profile["uuid"]], timeout=10)
    return done(removed=len(known))


def autoconnect(ssid, on):
    known = profiles_for(ssid)
    if not known:
        return fail("unknown")
    for profile in known:
        result = nm(["connection", "modify", "uuid", profile["uuid"], "connection.autoconnect", "yes" if on else "no"], timeout=10)
        if result.returncode:
            return fail(classify(result.stderr), result.stderr)
    return done()


def disconnect():
    device = wifi_device()
    if device:
        nm(["device", "disconnect", device], timeout=15)
    return done()


def radio(on):
    if on:
        set_rfkill("wlan", False)
    result = nm(["radio", "wifi", "on" if on else "off"], timeout=10)
    return fail(classify(result.stderr), result.stderr) if result.returncode else done()


def airplane(on):
    nm(["radio", "all", "off" if on else "on"], timeout=10)
    for kind in ("wlan", "bluetooth", "wwan"):
        set_rfkill(kind, on)
    if not on:
        bluetooth = shutil.which("bluetoothctl")
        if bluetooth:
            try: subprocess.run([bluetooth, "power", "on"], capture_output=True, timeout=5)
            except (OSError, subprocess.SubprocessError): pass
    return done(airplane=on)


def wps(ssid=""):
    """Bouton WPS : wpa_supplicant reçoit la clé de la box, puis profil NetworkManager."""
    device = wifi_device()
    if not device:
        return fail("no-device")
    try:
        from gi.repository import Gio, GLib
    except ImportError:
        return fail("wps-unsupported")
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SYSTEM, None)
        supplicant = Gio.DBusProxy.new_sync(bus, 0, None, "fi.w1.wpa_supplicant1", "/fi/w1/wpa_supplicant1",
                                            "fi.w1.wpa_supplicant1", None)
        path = supplicant.call_sync("GetInterface", GLib.Variant("(s)", (device,)), 0, 3000, None).unpack()[0]
        interface = Gio.DBusProxy.new_sync(bus, 0, None, "fi.w1.wpa_supplicant1", path,
                                           "fi.w1.wpa_supplicant1.Interface.WPS", None)
        properties = Gio.DBusProxy.new_sync(bus, 0, None, "fi.w1.wpa_supplicant1", path,
                                            "org.freedesktop.DBus.Properties", None)
        # La clé reçue revient ici : NetworkManager garde la main sur la connexion.
        properties.call_sync("Set", GLib.Variant("(ssv)", ("fi.w1.wpa_supplicant1.Interface.WPS", "ProcessCredentials",
                                                          GLib.Variant("b", False))), 0, 3000, None)
    except Exception as error:
        return fail("wps-unsupported", str(error))
    loop = GLib.MainLoop()
    outcome = {"error": "wps-timeout"}

    def signal(_proxy, _sender, name, parameters):
        if name == "Credentials":
            values = parameters.unpack()[0]
            outcome["ssid"] = bytes(values.get("SSID") or b"").decode("utf-8", errors="replace")
            outcome["key"] = bytes(values.get("Key") or b"").decode("utf-8", errors="replace")
            outcome["auth"] = list(values.get("AuthType") or [])
            outcome["error"] = None
            loop.quit()
        elif name == "Event":
            event = parameters.unpack()[0]
            if event == "pbc-overlap":
                outcome["error"] = "wps-overlap"
                loop.quit()
            elif event == "fail" and outcome.get("error") == "wps-timeout":
                outcome["error"] = "wps-failed"

    interface.connect("g-signal", signal)
    try:
        interface.call_sync("Start", GLib.Variant("(a{sv})", ({"Role": GLib.Variant("s", "enrollee"),
                                                               "Type": GLib.Variant("s", "pbc")},)), 0, 5000, None)
    except Exception as error:
        return fail("wps-unsupported", str(error))
    GLib.timeout_add_seconds(WPS_WAIT, loop.quit)
    loop.run()
    try: interface.call_sync("Cancel", None, 0, 3000, None)
    except Exception: pass
    if outcome["error"]:
        return fail(outcome["error"])
    security = "wpa" if outcome.get("key") else "open"
    return connect(outcome["ssid"] or ssid, security, outcome.get("key", ""))


def status():
    present = hardware_present()
    running = nm(["-t", "-f", "RUNNING", "general"], timeout=5).stdout.strip() == "running"
    device = wifi_device() if running else ""
    current = ""
    if running and device:
        for network in scan(False):
            if network["in_use"]:
                current = network["ssid"]
                break
    on = running and radio_on()
    return done(present=present, nm=running, device=device, radio=on, connected=current,
                airplane=running and airplane_state())


def backend_main(argv):
    if not argv:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    command, rest = argv[0], argv[1:]
    if command == "status": return status()
    if command == "list":
        rescan = "--rescan" in rest
        networks = scan(rescan)
        known = {p["ssid"] for p in profiles()}
        device = wifi_device()
        wps_names = wps_bsses(device) if device else None
        for network in networks:
            network["known"] = network["ssid"] in known
            network["wps"] = bool(wps_names and network["ssid"] in wps_names)
        return done(networks=networks)
    if command == "known": return done(networks=profiles())
    if command == "connect" and rest:
        ssid, options = rest[0], rest[1:]
        security, identity, hidden, auto, key = "wpa", "", False, True, ""
        it = iter(options)
        for option in it:
            if option == "--security": security = next(it, "")
            elif option == "--identity": identity = next(it, "")
            elif option == "--hidden": hidden = True
            elif option == "--no-auto": auto = False
            elif option == "--key-stdin": key = sys.stdin.readline().rstrip("\n")
            else: return fail("failed", "option " + option)
        return connect(ssid, security, key, identity, hidden, auto)
    if command == "up" and len(rest) == 1: return up(rest[0])
    if command == "forget" and len(rest) == 1: return forget(rest[0])
    if command == "autoconnect" and len(rest) == 2 and rest[1] in ("on", "off"): return autoconnect(rest[0], rest[1] == "on")
    if command == "disconnect": return disconnect()
    if command == "radio" and rest in (["on"], ["off"]): return radio(rest[0] == "on")
    if command == "airplane" and rest in (["on"], ["off"]): return airplane(rest[0] == "on")
    if command == "wps": return wps(rest[0] if rest else "")
    print(__doc__.strip(), file=sys.stderr)
    return 2


# ── Côté bureau : appel du service, en root par le relais ───────────────────

def backend_command(args):
    """Commande à lancer : directe en root (session Live, tests), sinon par winlux-root."""
    here = os.path.abspath(__file__)
    direct = os.environ.get("WINLUX_WIFI_DIRECT") == "1" or os.geteuid() == 0 or not os.path.exists(RELAY_SOCKET)
    if direct:
        return [sys.executable, "-B", here, *args]
    return [shutil.which("winlux-root") or "/usr/bin/winlux-root", "exec", "winlux-wifi", *args]


def call(args, key=None, timeout=200):
    """Lance le service ; renvoie son JSON (dict), avec ok=False et error en cas d'échec."""
    try:
        result = subprocess.run(backend_command(args), input=(key + "\n") if key is not None else "",
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
    "bad-key": "La clé de sécurité est incorrecte. Vérifiez-la (majuscules comprises) et réessayez.",
    "key-format": "Une clé WPA compte de 8 à 63 caractères (ou 64 chiffres hexadécimaux).",
    "wep-format": "Une clé WEP compte 5 ou 13 caractères (ou 10 ou 26 chiffres hexadécimaux).",
    "eap-identity": "Entrez votre nom d’utilisateur et votre mot de passe.",
    "out-of-range": "Le réseau « {name} » est hors de portée. Rapprochez-vous de la box ou vérifiez le nom du réseau.",
    "unknown": "Le réseau « {name} » est hors de portée. Rapprochez-vous de la box ou vérifiez le nom du réseau.",
    "timeout": "La box n’a pas répondu à temps. Réessayez ; si cela se reproduit, redémarrez la box.",
    "no-ip": "Le Wi-Fi est relié, mais la box n’a pas donné d’adresse à cet ordinateur. Redémarrez la box ou réessayez.",
    "wifi-off": "Le Wi-Fi est désactivé. Activez-le (et quittez le mode avion), puis réessayez.",
    "no-service": "Le service réseau (NetworkManager) ne répond pas. Redémarrez l’ordinateur.",
    "no-device": "Aucune carte Wi-Fi utilisable n’a été trouvée.",
    "wps-timeout": "Aucune box n’a répondu au WPS en 2 minutes. Appuyez sur le bouton WPS de la box, puis réessayez.",
    "wps-failed": "Aucune box n’a répondu au WPS en 2 minutes. Appuyez sur le bouton WPS de la box, puis réessayez.",
    "wps-overlap": "Plusieurs box sont en mode WPS en même temps. Attendez deux minutes et réessayez.",
    "wps-unsupported": "Cette carte Wi-Fi ne gère pas le WPS : entrez la clé de sécurité.",
    "relay": "Le service système de Winlux ne répond pas. Redémarrez l’ordinateur.",
    "not-authorized": "Le service système de Winlux ne répond pas. Redémarrez l’ordinateur.",
}
# Erreurs qui se corrigent dans la boîte de la clé (elle se rouvre avec le message).
KEY_ERRORS = ("bad-key", "key-format", "wep-format", "eap-identity")


def error_text(answer, ssid=""):
    code = answer.get("error", "failed")
    text = _t(ERRORS.get(code, "La connexion a échoué.")).format(name=ssid)
    detail = answer.get("detail", "")
    if code not in ERRORS and detail:
        text += "\n" + _t("Détail : {detail}").format(detail=detail.splitlines()[-1][:200])
    return text


def signal_level(signal):
    if signal >= 75: return "excellent", "network-wireless-signal-excellent-symbolic"
    if signal >= 50: return "bon", "network-wireless-signal-good-symbolic"
    if signal >= 25: return "moyen", "network-wireless-signal-ok-symbolic"
    return "faible", "network-wireless-signal-weak-symbolic"


def osk_module():
    """Clavier tactile de Winlux (winlux-osk, installé à côté), ou None."""
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    for candidate in (here.with_name("winlux-osk.py"), here.with_name("winlux-osk"), Path("/usr/bin/winlux-osk")):
        if candidate.is_file():
            try:
                loader = SourceFileLoader("winlux_osk_wifi", str(candidate))
                spec = importlib.util.spec_from_loader("winlux_osk_wifi", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                return module
            except Exception:
                return None
    return None


def physical_keyboard(module):
    if os.environ.get("WINLUX_WIFI_OSK") == "1": return False
    try: return bool(module.keyboards())
    except Exception: return True


def available():
    """La page n'existe qu'avec une carte Wi-Fi et NetworkManager."""
    return hardware_present() and shutil.which("nmcli") is not None


def page(host):
    return WifiPanel(host)


class WifiPanel:
    """Page Wi-Fi de Paramètres, construite dans `host` (section, card, row, hint…)."""

    def __init__(self, host):
        import gi
        gi.require_version("Gtk", "3.0")
        from gi.repository import GLib, Gtk
        self.Gtk, self.GLib = Gtk, GLib
        self.host = host
        self.busy = False
        self.state = {}
        self.networks = []
        self.known = []
        box = host.card()
        self.radio_switch = host.switch(box, _t("Wi-Fi"), True, self.toggle_radio, " ")
        self.state_label = self.subtitle_of(self.radio_switch)
        self.airplane_switch = host.switch(box, _t("Mode avion"), False, self.toggle_airplane,
                                           _t("Coupe le Wi-Fi, le Bluetooth et le réseau mobile."))
        host.section(_t("Réseaux disponibles"),
                     _t("Cliquez sur un réseau pour vous y connecter. Winlux le retient et s’y reconnecte seul au démarrage."))
        self.list_card = host.card()
        self.message = Gtk.Label(xalign=0, wrap=True, selectable=True)
        self.message.set_name("wifi-message")
        self.list_card.pack_start(self.message, False, False, 0)
        self.network_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.list_card.pack_start(self.network_box, False, False, 0)
        self.list_actions = host.actions(self.list_card, [(_t("Actualiser"), lambda: self.refresh(True)),
                                                          (_t("Ajouter un réseau caché…"), self.hidden_dialog)])
        host.section(_t("Réseaux enregistrés"))
        self.known_card = host.card()
        self.updating = False
        self.say(_t("Recherche des réseaux…"))
        GLib.idle_add(lambda: self.refresh(False) and False)

    def subtitle_of(self, widget):
        line = widget.get_parent()
        texts = line.get_children()[0]
        labels = texts.get_children()
        return labels[1] if len(labels) > 1 else labels[0]

    def say(self, text, error=False):
        self.message.set_text(text)
        context = self.message.get_style_context()
        (context.add_class if error else context.remove_class)("error")
        self.message.set_visible(bool(text))

    def run(self, args, done, key=None):
        """Service en arrière-plan ; done(réponse) dans la boucle GTK."""
        import threading
        self.busy = True

        def worker():
            answer = call(args, key)
            self.GLib.idle_add(lambda: (setattr(self, "busy", False), done(answer)) and False)
        threading.Thread(target=worker, daemon=True).start()

    def refresh(self, rescan):
        if self.busy: return False
        if rescan: self.say(_t("Recherche des réseaux…"))
        self.run(["status"], lambda state: self.got_status(state, rescan))
        return False

    def got_status(self, state, rescan):
        self.state = state if state.get("ok") else {}
        self.updating = True
        on = bool(self.state.get("radio"))
        self.radio_switch.set_active(on)
        self.airplane_switch.set_active(bool(self.state.get("airplane")))
        self.updating = False
        connected = self.state.get("connected")
        self.state_label.set_text(_t("Connecté à « {name} »").format(name=connected) if connected and on
                                  else _t("Non connecté") if on else _t("Désactivé"))
        if not state.get("ok"):
            self.say(error_text(state), True)
            return
        if not on:
            self.networks = []
            self.show_networks()
            self.say(_t("Activez le Wi-Fi pour voir les réseaux à proximité."))
            self.run(["known"], self.got_known)
            return
        self.run(["list", "--rescan"] if rescan else ["list"], self.got_list)

    def got_list(self, answer):
        if not answer.get("ok"):
            self.say(error_text(answer), True)
        else:
            self.networks = answer.get("networks", [])
            self.say("" if self.networks else _t("Aucun réseau Wi-Fi à portée."))
        self.show_networks()
        self.run(["known"], self.got_known)

    def got_known(self, answer):
        self.known = answer.get("networks", []) if answer.get("ok") else []
        self.show_known()

    def network_subtitle(self, network):
        level, _ = signal_level(network.get("signal", 0))
        security = network.get("security", "open")
        parts = [_t("Ouvert") if security in ("open", "owe") else
                 (_t("Entreprise") if security == "eap" else _t("Sécurisé")) +
                 (f" ({network['security_text']})" if network.get("security_text") else "")]
        parts.append(_t("Signal : {level}").format(level=_t(level)) + f" ({network.get('signal', 0)} %)")
        if network.get("known"): parts.append(_t("Enregistré"))
        if network.get("wps"): parts.append(_t("WPS"))
        return " · ".join(parts)

    def show_networks(self):
        Gtk = self.Gtk
        for child in self.network_box.get_children(): child.destroy()
        for network in self.networks[:30]:
            _, icon = signal_level(network.get("signal", 0))
            line = Gtk.Box(spacing=12)
            theme = Gtk.IconTheme.get_default()
            image = Gtk.Image.new_from_icon_name(icon if theme.has_icon(icon) else "network-wireless-symbolic",
                                                 Gtk.IconSize.LARGE_TOOLBAR)
            line.pack_start(image, False, False, 0)
            texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, valign=Gtk.Align.CENTER)
            name = Gtk.Label(label=network["ssid"], xalign=0, wrap=True)
            name.get_style_context().add_class("wifi-name")
            texts.pack_start(name, False, False, 0)
            self.host.hint(self.network_subtitle(network), texts)
            line.pack_start(texts, True, True, 0)
            if network.get("in_use"):
                button = Gtk.Button(label=_t("Déconnecter"))
                button.connect("clicked", lambda *_: self.disconnect())
            else:
                button = Gtk.Button(label=_t("Se connecter"))
                button.connect("clicked", lambda *_, n=network: self.choose(n))
                button.get_style_context().add_class("suggested-action")
            button.set_valign(Gtk.Align.CENTER)
            button.get_accessible().set_name(f"{button.get_label()} {network['ssid']}")
            line.pack_start(button, False, False, 0)
            self.network_box.pack_start(line, False, False, 0)
        self.network_box.show_all()

    def show_known(self):
        Gtk = self.Gtk
        for child in self.known_card.get_children(): child.destroy()
        if not self.known:
            self.host.hint(_t("Aucun réseau enregistré."), self.known_card)
        for profile in self.known:
            line = Gtk.Box(spacing=8)
            switch = Gtk.Switch(active=profile.get("autoconnect", True), valign=Gtk.Align.CENTER)
            switch.set_tooltip_text(_t("Se connecter automatiquement"))
            switch.get_accessible().set_name(_t("Se connecter automatiquement"))
            switch.connect("notify::active", lambda widget, _, p=profile: self.set_auto(p["ssid"], widget.get_active()))
            forget = Gtk.Button(label=_t("Oublier"), valign=Gtk.Align.CENTER)
            forget.connect("clicked", lambda *_, p=profile: self.forget(p["ssid"]))
            line.pack_start(Gtk.Label(label=_t("Se connecter automatiquement"), xalign=1), False, False, 0)
            line.pack_start(switch, False, False, 0)
            line.pack_start(forget, False, False, 0)
            self.host.row(self.known_card, profile["ssid"], line, _t("Connecté") if profile.get("active") else None)
        self.known_card.show_all()

    # ── Actions ──

    def toggle_radio(self, on):
        if self.updating: return
        if on: self.clear_airplane_marker()
        self.run(["radio", "on" if on else "off"], lambda answer: self.after(answer, rescan=on))

    def toggle_airplane(self, on):
        if self.updating: return
        try:
            if on:
                AIRPLANE.parent.mkdir(parents=True, exist_ok=True)
                AIRPLANE.touch(mode=0o600)
            else:
                self.clear_airplane_marker()
        except OSError:
            pass
        self.run(["airplane", "on" if on else "off"], lambda answer: self.after(answer, rescan=not on))

    def clear_airplane_marker(self):
        try: AIRPLANE.unlink()
        except OSError: pass

    def after(self, answer, rescan=False, ssid=""):
        if not answer.get("ok"):
            self.say(error_text(answer, ssid), True)
            self.busy = False
            self.refresh(False)
            return
        self.busy = False
        self.refresh(rescan)

    def disconnect(self):
        self.run(["disconnect"], self.after)

    def set_auto(self, ssid, on):
        self.run(["autoconnect", ssid, "on" if on else "off"], lambda answer: self.after(answer, ssid=ssid))

    def forget(self, ssid):
        Gtk = self.Gtk
        dialog = Gtk.MessageDialog(transient_for=self.host, modal=True, message_type=Gtk.MessageType.QUESTION,
                                   buttons=Gtk.ButtonsType.NONE, text=_t("Oublier « {name} » ?").format(name=ssid))
        dialog.format_secondary_text(_t("La clé de sécurité sera effacée de cet ordinateur."))
        dialog.add_button(_t("Annuler"), Gtk.ResponseType.CANCEL)
        dialog.add_button(_t("Oublier"), Gtk.ResponseType.OK)
        answer = dialog.run()
        dialog.destroy()
        if answer == Gtk.ResponseType.OK:
            self.run(["forget", ssid], self.after)

    def choose(self, network):
        """Clic sur un réseau : connu -> connexion ; ouvert -> connexion ; sinon la clé."""
        ssid = network["ssid"]
        if network.get("known"):
            self.connecting(ssid)
            self.run(["up", ssid], lambda answer: self.connected(answer, network))
        elif network.get("security") in ("open", "owe"):
            self.connecting(ssid)
            self.run(["connect", ssid, "--security", network["security"]], lambda answer: self.connected(answer, network))
        else:
            self.key_dialog(network)

    def connecting(self, ssid):
        self.say(_t("Connexion à « {name} »…").format(name=ssid))

    def connected(self, answer, network, error=None):
        ssid = network["ssid"]
        if answer.get("ok"):
            self.say(_t("Connecté à « {name} ».").format(name=ssid))
            self.busy = False
            self.refresh(False)
            return
        if answer.get("error") in KEY_ERRORS or (answer.get("error") == "unknown" and network.get("security") not in ("open", "owe")):
            # Profil enregistré dont la clé ne marche plus, ou clé refusée : on la redemande.
            self.say("")
            self.key_dialog(dict(network, known=False), error_text(answer, ssid))
            return
        self.say(error_text(answer, ssid), True)
        self.refresh(False)

    def key_dialog(self, network, error=None, hidden=False):
        """Boîte de la clé, façon Windows 11 ; clavier tactile intégré sous le champ."""
        Gtk = self.Gtk
        ssid = network.get("ssid", "")
        dialog = Gtk.Dialog(title=_t("Réseau caché") if hidden else _t("Connexion à « {name} »").format(name=ssid),
                            transient_for=self.host, modal=True)
        dialog.set_role("winlux-wifi-key")
        dialog.add_button(_t("Annuler"), Gtk.ResponseType.CANCEL)
        next_button = dialog.add_button(_t("Suivant"), Gtk.ResponseType.OK)
        next_button.get_style_context().add_class("suggested-action")
        dialog.set_default_response(Gtk.ResponseType.OK)
        screen = self.host.get_screen()
        width = screen.get_width() if screen is not None else 1366
        dialog.set_default_size(min(720, max(360, width - 32)), -1)
        area = dialog.get_content_area()
        small = screen is not None and screen.get_height() < 700
        area.set_spacing(6 if small else 10)
        area.set_border_width(10 if small else 18)
        fields = {}
        security = network.get("security", "wpa")
        if hidden:
            fields["ssid"] = self.field(area, _t("Nom du réseau (SSID)"), visible=True)
            combo = Gtk.ComboBoxText()
            for key, label in (("wpa", "WPA2/WPA3 Personnel (le plus courant)"), ("sae", "WPA3 Personnel uniquement"),
                               ("eap", "WPA2/WPA3 Entreprise"), ("wep", "WEP (ancien, peu sûr)"),
                               ("open", "Aucune (réseau ouvert)")):
                combo.append(key, _t(label))
            combo.set_active_id("wpa")
            area.pack_start(Gtk.Label(label=_t("Type de sécurité"), xalign=0), False, False, 0)
            area.pack_start(combo, False, False, 0)
            fields["security"] = combo
        else:
            area.pack_start(Gtk.Label(label=_t("Entrez la clé de sécurité du réseau"), xalign=0, wrap=True), False, False, 0)
        identity_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        area.pack_start(identity_box, False, False, 0)
        fields["identity"] = self.field(identity_box, _t("Nom d’utilisateur"), visible=True)
        key_title = Gtk.Label(xalign=0)
        area.pack_start(key_title, False, False, 0)
        key = Gtk.Entry(visibility=False, activates_default=True)
        key.set_input_purpose(Gtk.InputPurpose.PASSWORD)
        key.set_name("wifi-key")
        area.pack_start(key, False, False, 0)
        fields["key"] = key
        show = Gtk.CheckButton(label=_t("Afficher les caractères"))
        show.connect("toggled", lambda button: key.set_visibility(button.get_active()))
        area.pack_start(show, False, False, 0)
        auto = Gtk.CheckButton(label=_t("Se connecter automatiquement"), active=True)
        area.pack_start(auto, False, False, 0)
        problem = Gtk.Label(xalign=0, wrap=True)
        problem.get_style_context().add_class("error")
        problem.set_name("wifi-key-error")
        problem.set_no_show_all(True)
        area.pack_start(problem, False, False, 0)

        def show_problem(text):
            problem.set_text(text or "")
            problem.set_visible(bool(text))
        show_problem(error)
        wps_button = None
        if network.get("wps") and not hidden:
            area.pack_start(Gtk.Label(label=_t("Vous pouvez aussi vous connecter en appuyant sur le bouton WPS de votre box."),
                                      xalign=0, wrap=True), False, False, 0)
            wps_button = Gtk.Button(label=_t("Utiliser le bouton WPS"), halign=Gtk.Align.START)
            area.pack_start(wps_button, False, False, 0)

        def current_security():
            return fields["security"].get_active_id() if hidden else security

        def update(*_):
            kind = current_security()
            identity_box.set_visible(kind == "eap")
            key_title.set_text(_t("Mot de passe") if kind == "eap" else _t("Clé de sécurité"))
            key.set_sensitive(kind not in ("open", "owe"))
            ssid_ok = (not hidden) or bool(fields["ssid"].get_text())
            text = key.get_text()
            next_button.set_sensitive(ssid_ok and (kind in ("open", "owe") or
                                                   key_problem(kind, text, fields["identity"].get_text() or "x") is None))
        for widget in (key, fields["identity"], *( [fields["ssid"]] if hidden else [])):
            widget.connect("changed", update)
        if hidden: fields["security"].connect("changed", update)
        # Clavier tactile dans la boîte même : le champ reste toujours au-dessus.
        osk = osk_module()
        keyboard_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        keyboard_box.set_no_show_all(True)
        keyboard_box.set_vexpand(False)
        target = {"entry": key}

        def remember(widget, _event=None):
            target["entry"] = widget
            return False
        for widget in (key, fields["identity"], *([fields["ssid"]] if hidden else [])):
            widget.connect("focus-in-event", remember)

        def touch_key(name, _modifiers=()):
            entry = target["entry"]
            if name == "Return":
                if next_button.get_sensitive(): dialog.response(Gtk.ResponseType.OK)
                return
            if name == "Escape":
                keyboard_box.hide()
                return
            if name == "Tab":
                dialog.child_focus(Gtk.DirectionType.TAB_FORWARD)
                return
            position = entry.get_position()
            bounds = entry.get_selection_bounds()
            if name == "BackSpace":
                if bounds: entry.delete_text(*bounds)
                elif position > 0: entry.delete_text(position - 1, position)
            elif name in ("Left", "Right"):
                entry.set_position(max(0, position + (1 if name == "Right" else -1)))
            elif len(name) == 1:
                if bounds:
                    entry.delete_text(*bounds)
                    position = bounds[0]
                entry.insert_text(name, position)
                entry.set_position(position + 1)
            entry.grab_focus_without_selecting()

        if osk is not None:
            try:
                widget = osk.keyboard_widget(send=touch_key, on_close=keyboard_box.hide)
                height = screen.get_height() if screen is not None else 768
                widget.set_size_request(-1, max(180, min(270, int(height * 0.32))))
                for button in self.buttons_in(widget): button.set_can_focus(False)
                keyboard_box.pack_start(widget, False, False, 0)
                self.keyboard_css()
                toggle = Gtk.Button(label="⌨ " + _t("Clavier tactile"), can_focus=False)
                toggle.connect("clicked", lambda *_: keyboard_box.hide() if keyboard_box.get_visible()
                               else (widget.show_all(), keyboard_box.show()))
                # À côté d'Annuler/Suivant : la boîte tient en 1024x600 avec le clavier.
                actions = dialog.get_action_area()
                actions.pack_start(toggle, False, False, 0)
                actions.set_child_secondary(toggle, True)
                area.pack_end(keyboard_box, False, False, 0)
                if not physical_keyboard(osk):
                    widget.show_all()
                    keyboard_box.show()
            except Exception:
                pass
        dialog.show_all()
        update()
        (fields["ssid"] if hidden else key).grab_focus()
        if wps_button is not None:
            wps_button.connect("clicked", lambda *_: dialog.response(1))
        while True:
            response = dialog.run()
            if response == Gtk.ResponseType.OK:
                kind = current_security()
                problem_code = key_problem(kind, key.get_text(), fields["identity"].get_text())
                if problem_code:
                    show_problem(error_text({"error": problem_code}))
                    continue
            break
        values = {"ssid": fields["ssid"].get_text() if hidden else ssid, "security": current_security(),
                  "key": key.get_text(), "identity": fields["identity"].get_text(), "auto": auto.get_active()}
        dialog.destroy()
        if response == 1:
            self.say(_t("Appuyez maintenant sur le bouton WPS de votre box (vous avez 2 minutes)…"))
            self.run(["wps", ssid], lambda answer: self.connected(answer, network))
            return
        if response != Gtk.ResponseType.OK:
            return
        args = ["connect", values["ssid"], "--security", values["security"]]
        if hidden: args.append("--hidden")
        if not values["auto"]: args.append("--no-auto")
        if values["security"] == "eap": args += ["--identity", values["identity"]]
        secret = None
        if values["security"] not in ("open", "owe"):
            args.append("--key-stdin")
            secret = values["key"]
        target_network = dict(network, ssid=values["ssid"], security=values["security"])
        self.connecting(values["ssid"])
        self.run(args, lambda answer: self.connected_hidden(answer, target_network) if hidden
                 else self.connected(answer, target_network), key=secret)

    def connected_hidden(self, answer, network):
        if answer.get("ok") or answer.get("error") not in KEY_ERRORS:
            self.connected(answer, network)
        else:
            self.say(error_text(answer, network["ssid"]), True)
            self.busy = False

    def keyboard_css(self):
        """Touches du clavier tactile : le style des boutons de Paramètres
        (marges, bordure violette) les élargissait au-delà d'un écran 1024x600."""
        if getattr(WifiPanel, "_keyboard_css", None) is not None: return
        from gi.repository import Gdk
        provider = self.Gtk.CssProvider()
        provider.load_from_data(b"""
#winlux-osk button.osk-key { background: #353b4d; color: #f2f3f8; border: none; border-radius: 6px;
    padding: 0; min-width: 0; min-height: 0; font-size: 17px; box-shadow: none; background-image: none; }
#winlux-osk button.osk-key label { color: #f2f3f8; }
#winlux-osk button.osk-key:hover { background: #454c62; }
#winlux-osk button.osk-key:active { background: #5a6280; }
#winlux-osk button.osk-special { background: #2a2f3e; font-size: 15px; }
#winlux-osk button.osk-active, #winlux-osk button.osk-active label { background: #8178b7; color: #ffffff; }
""")
        screen = Gdk.Screen.get_default()
        if screen is not None:
            self.Gtk.StyleContext.add_provider_for_screen(screen, provider, self.Gtk.STYLE_PROVIDER_PRIORITY_USER + 2)
        WifiPanel._keyboard_css = provider

    def buttons_in(self, widget):
        Gtk = self.Gtk
        if isinstance(widget, Gtk.Button):
            yield widget
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children():
                yield from self.buttons_in(child)

    def field(self, parent, title, visible=True):
        Gtk = self.Gtk
        parent.pack_start(Gtk.Label(label=title, xalign=0), False, False, 0)
        entry = Gtk.Entry(visibility=visible, activates_default=True)
        parent.pack_start(entry, False, False, 0)
        return entry

    def hidden_dialog(self):
        self.key_dialog({"ssid": "", "security": "wpa"}, hidden=True)


if __name__ == "__main__":
    raise SystemExit(backend_main(sys.argv[1:]))
