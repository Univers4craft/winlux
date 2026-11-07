#!/usr/bin/python3
"""Téléphone connecté : relier Winlux à un téléphone Android.

Comme « Lien avec un téléphone » sous Windows 11 : notifications du téléphone
sur le PC, SMS lus et envoyés depuis le PC, fichiers échangés dans les deux
sens, batterie et « Faire sonner ». Côté téléphone, l'application libre
KDE Connect (F-Droid ou Google Play) sert de client : Winlux parle son
protocole ouvert (version 8), écrit entièrement ici, sans bibliothèque KDE.

Protocole, en bref :
  * découverte : paquet JSON « kdeconnect.identity » en UDP sur le port 1716 ;
  * liaison : le téléphone ouvre une connexion TCP vers Winlux, envoie son
    identité en clair, puis TLS (Winlux client, certificats auto-signés de
    part et d'autre, identité renvoyée sous TLS en version 8) ;
  * appairage : « kdeconnect.pair », code de vérification affiché des deux
    côtés ; le certificat du téléphone est ensuite épinglé ;
  * fichiers : connexion TLS séparée sur un port 1739-1764.
Winlux est toujours client TLS sur la liaison principale : il répond aux
annonces du téléphone par sa propre annonce, et c'est le téléphone qui se
connecte. Le pare-feu Winlux n'ouvre ces ports que sur les réseaux privés et
de domaine ; sur un réseau public, Winlux ne s'annonce pas.

Usage :
  winlux-phone serve [--if-paired]      service de la session (un par compte)
  winlux-phone gui [notifications|messages|files [CONVERSATION]]
                                        application « Téléphone connecté »
  winlux-phone status                   état (JSON)
  winlux-phone pair|accept|reject|unpair ID
  winlux-phone ring ID                  faire sonner le téléphone
  winlux-phone send ID FICHIER…         envoyer des fichiers au téléphone
  winlux-phone sms ID NUMÉRO TEXTE      envoyer un SMS
  winlux-phone set ID notifications|sms|files on|off
  winlux-phone enable|disable           autoriser ou couper la connexion
  winlux-phone pair-dialog ID           fenêtre de confirmation d'appairage
  winlux-phone reply ID RÉPONSE         fenêtre de réponse à une notification
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import select
import shutil
import socket
import ssl
import subprocess
import sys
import threading
import time
import uuid


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
    'Téléphone connecté': ('Connected phone', 'Teléfono conectado', 'Verbundenes Telefon'),
    'Associer un téléphone': ('Pair a phone', 'Vincular un teléfono', 'Telefon koppeln'),
    'Associer': ('Pair', 'Vincular', 'Koppeln'),
    'Accepter': ('Accept', 'Aceptar', 'Annehmen'),
    'Refuser': ('Decline', 'Rechazar', 'Ablehnen'),
    'Dissocier': ('Unpair', 'Desvincular', 'Entkoppeln'),
    '{name} veut s’associer à ce PC': ('{name} wants to pair with this PC', '{name} quiere vincularse con este PC', '{name} möchte sich mit diesem PC koppeln'),
    'Vérifiez que le téléphone affiche le même code : {key}': ('Check that the phone shows the same code: {key}', 'Compruebe que el teléfono muestra el mismo código: {key}', 'Prüfen Sie, ob das Telefon denselben Code anzeigt: {key}'),
    'Téléphone associé': ('Phone paired', 'Teléfono vinculado', 'Telefon gekoppelt'),
    '{name} est maintenant relié à ce PC.': ('{name} is now linked to this PC.', '{name} ya está vinculado a este PC.', '{name} ist jetzt mit diesem PC verbunden.'),
    'Téléphone dissocié': ('Phone unpaired', 'Teléfono desvinculado', 'Telefon entkoppelt'),
    '{name} n’est plus relié à ce PC.': ('{name} is no longer linked to this PC.', '{name} ya no está vinculado a este PC.', '{name} ist nicht mehr mit diesem PC verbunden.'),
    'Fichier reçu de {name}': ('File received from {name}', 'Archivo recibido de {name}', 'Datei von {name} empfangen'),
    'Enregistré dans {folder}': ('Saved to {folder}', 'Guardado en {folder}', 'Gespeichert in {folder}'),
    'Ouvrir le dossier': ('Open folder', 'Abrir carpeta', 'Ordner öffnen'),
    'Texte reçu de {name}': ('Text received from {name}', 'Texto recibido de {name}', 'Text von {name} empfangen'),
    'Lien reçu de {name}': ('Link received from {name}', 'Enlace recibido de {name}', 'Link von {name} empfangen'),
    'Ouvrir': ('Open', 'Abrir', 'Öffnen'),
    'Répondre': ('Reply', 'Responder', 'Antworten'),
    'Ignorer': ('Dismiss', 'Descartar', 'Verwerfen'),
    'Appel entrant': ('Incoming call', 'Llamada entrante', 'Eingehender Anruf'),
    'Appel manqué': ('Missed call', 'Llamada perdida', 'Verpasster Anruf'),
    'Couper la sonnerie': ('Mute ringtone', 'Silenciar tono', 'Klingelton stumm'),
    'Batterie faible': ('Low battery', 'Batería baja', 'Akku schwach'),
    '{name} : {level} % restants': ('{name}: {level}% left', '{name}: {level} % restante', '{name}: {level} % verbleibend'),
    'Ping de {name}': ('Ping from {name}', 'Ping de {name}', 'Ping von {name}'),
    '{name} cherche ce PC': ('{name} is looking for this PC', '{name} está buscando este PC', '{name} sucht diesen PC'),
    'Envoi de fichier impossible': ('Couldn’t send the file', 'No se pudo enviar el archivo', 'Datei konnte nicht gesendet werden'),
    'Le téléphone ne s’est pas connecté pour recevoir {file}.': ('The phone didn’t connect to receive {file}.', 'El teléfono no se conectó para recibir {file}.', 'Das Telefon hat sich nicht verbunden, um {file} zu empfangen.'),
    'Fichier envoyé': ('File sent', 'Archivo enviado', 'Datei gesendet'),
    '{file} est sur {name}.': ('{file} is on {name}.', '{file} está en {name}.', '{file} ist auf {name}.'),
    # Application
    'Notifications': ('Notifications', 'Notificaciones', 'Benachrichtigungen'),
    'Messages': ('Messages', 'Mensajes', 'Nachrichten'),
    'Fichiers': ('Files', 'Archivos', 'Dateien'),
    'Connecté': ('Connected', 'Conectado', 'Verbunden'),
    'Hors de portée': ('Not reachable', 'Fuera de alcance', 'Nicht erreichbar'),
    'en charge': ('charging', 'cargando', 'wird geladen'),
    'Faire sonner': ('Ring', 'Hacer sonar', 'Klingeln lassen'),
    'Envoyer un fichier…': ('Send a file…', 'Enviar un archivo…', 'Datei senden…'),
    'Envoyer': ('Send', 'Enviar', 'Senden'),
    'Annuler': ('Cancel', 'Cancelar', 'Abbrechen'),
    'Nouveau message': ('New message', 'Mensaje nuevo', 'Neue Nachricht'),
    'Numéro de téléphone': ('Phone number', 'Número de teléfono', 'Telefonnummer'),
    'Écrire un SMS': ('Write a text message', 'Escribir un SMS', 'SMS schreiben'),
    'Écrire une réponse': ('Write a reply', 'Escribir una respuesta', 'Antwort schreiben'),
    'Aucune notification du téléphone.': ('No notifications from the phone.', 'No hay notificaciones del teléfono.', 'Keine Benachrichtigungen vom Telefon.'),
    'Aucune conversation pour l’instant. Autorisez l’accès aux SMS dans KDE Connect sur le téléphone.': ('No conversations yet. Allow SMS access in KDE Connect on the phone.', 'Todavía no hay conversaciones. Permita el acceso a los SMS en KDE Connect en el teléfono.', 'Noch keine Unterhaltungen. Erlauben Sie den SMS-Zugriff in KDE Connect auf dem Telefon.'),
    'Choisissez une conversation.': ('Choose a conversation.', 'Elija una conversación.', 'Wählen Sie eine Unterhaltung.'),
    'Fichiers reçus': ('Received files', 'Archivos recibidos', 'Empfangene Dateien'),
    'Aucun fichier reçu pour l’instant. Sur le téléphone, choisissez Partager › KDE Connect › ce PC.': ('No files received yet. On the phone, choose Share › KDE Connect › this PC.', 'Todavía no se ha recibido ningún archivo. En el teléfono, elija Compartir › KDE Connect › este PC.', 'Noch keine Dateien empfangen. Wählen Sie auf dem Telefon Teilen › KDE Connect › diesen PC.'),
    'Reliez votre téléphone Android à ce PC': ('Link your Android phone to this PC', 'Vincule su teléfono Android a este PC', 'Verbinden Sie Ihr Android-Telefon mit diesem PC'),
    '1. Installez l’application libre KDE Connect sur le téléphone (F-Droid ou Google Play).': ('1. Install the free KDE Connect app on the phone (F-Droid or Google Play).', '1. Instale la aplicación libre KDE Connect en el teléfono (F-Droid o Google Play).', '1. Installieren Sie die freie App KDE Connect auf dem Telefon (F-Droid oder Google Play).'),
    '2. Connectez le téléphone au même réseau Wi-Fi que ce PC (réseau privé).': ('2. Connect the phone to the same Wi-Fi network as this PC (private network).', '2. Conecte el teléfono a la misma red Wi-Fi que este PC (red privada).', '2. Verbinden Sie das Telefon mit demselben WLAN wie diesen PC (privates Netzwerk).'),
    '3. Ouvrez KDE Connect : ce PC apparaît sous le nom « {name} ». Associez-le depuis le téléphone ou ci-dessous.': ('3. Open KDE Connect: this PC shows up as “{name}”. Pair it from the phone or below.', '3. Abra KDE Connect: este PC aparece como «{name}». Vincúlelo desde el teléfono o aquí abajo.', '3. Öffnen Sie KDE Connect: Dieser PC erscheint als „{name}“. Koppeln Sie ihn am Telefon oder hier unten.'),
    'Téléphones à proximité': ('Nearby phones', 'Teléfonos cercanos', 'Telefone in der Nähe'),
    'Recherche des téléphones sur le réseau…': ('Looking for phones on the network…', 'Buscando teléfonos en la red…', 'Telefone im Netzwerk werden gesucht…'),
    'Rechercher à nouveau': ('Search again', 'Buscar de nuevo', 'Erneut suchen'),
    'Demande envoyée : acceptez-la sur le téléphone.': ('Request sent: accept it on the phone.', 'Solicitud enviada: acéptela en el teléfono.', 'Anfrage gesendet: Bestätigen Sie sie auf dem Telefon.'),
    'Ce réseau est public : le téléphone ne peut pas trouver ce PC. Passez le réseau en privé dans Paramètres › Réseau et Internet.': ('This network is public: the phone can’t find this PC. Make the network private in Settings › Network & internet.', 'Esta red es pública: el teléfono no puede encontrar este PC. Cambie la red a privada en Configuración › Red e Internet.', 'Dieses Netzwerk ist öffentlich: Das Telefon kann diesen PC nicht finden. Stellen Sie es unter Einstellungen › Netzwerk und Internet auf privat.'),
    'Le service Téléphone connecté ne répond pas.': ('The Connected phone service isn’t responding.', 'El servicio Teléfono conectado no responde.', 'Der Dienst „Verbundenes Telefon“ antwortet nicht.'),
    'Ajouter un téléphone': ('Add a phone', 'Añadir un teléfono', 'Telefon hinzufügen'),
    'Ouvrir Téléchargements': ('Open Downloads', 'Abrir Descargas', 'Downloads öffnen'),
    'Sur le téléphone, autorisez KDE Connect à lire les notifications.': ('On the phone, allow KDE Connect to read notifications.', 'En el teléfono, permita que KDE Connect lea las notificaciones.', 'Erlauben Sie KDE Connect auf dem Telefon, Benachrichtigungen zu lesen.'),
    'Moi': ('Me', 'Yo', 'Ich'),
    'Vous : ': ('You: ', 'Usted: ', 'Sie: '),
    'Batterie {level} %': ('Battery {level}%', 'Batería {level} %', 'Akku {level} %'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# ── Emplacements et constantes ─────────────────────────────────────────────

HOME = Path.home()
CONFIG = Path(os.environ.get("WINLUX_PHONE_CONFIG", HOME / ".config/winlux/phone"))
DATA = Path(os.environ.get("WINLUX_PHONE_DATA", HOME / ".local/state/winlux/phone"))
_runtime = os.environ.get("XDG_RUNTIME_DIR")
RUNTIME = Path(os.environ.get("WINLUX_PHONE_RUNTIME") or (Path(_runtime) / "winlux-phone" if _runtime else HOME / ".cache/winlux/phone"))
SOCKET = RUNTIME / "control.sock"
STATUS = RUNTIME / "status.json"
DEVICES = CONFIG / "devices.json"
SETTINGS = CONFIG / "settings.json"
CERT = CONFIG / "certificate.pem"
KEY = CONFIG / "private-key.pem"
FIREWALL_CONF = Path(os.environ.get("WINLUX_FIREWALL_CONF", "/etc/winlux/firewall.conf"))

PROTOCOL_VERSION = 8
PORT = int(os.environ.get("WINLUX_PHONE_PORT", "1716"))
REMOTE_PORT = int(os.environ.get("WINLUX_PHONE_REMOTE_PORT", "1716"))
_payload = int(os.environ.get("WINLUX_PHONE_PAYLOAD_PORT", "1739"))
PAYLOAD_PORTS = range(_payload, _payload + 26)
MAX_LINE = 32 * 1024 * 1024
PAIR_TIMEOUT = 30
DEVICE_ID = re.compile(r"^[A-Za-z0-9_-]{16,64}$")
FIREWALL_RULES = ("9447", "9448")

INCOMING = ["kdeconnect.ping", "kdeconnect.notification", "kdeconnect.sms.messages",
            "kdeconnect.share.request", "kdeconnect.battery", "kdeconnect.telephony",
            "kdeconnect.contacts.response_uids_timestamps", "kdeconnect.contacts.response_vcards",
            "kdeconnect.findmyphone.request"]
OUTGOING = ["kdeconnect.ping", "kdeconnect.notification.request", "kdeconnect.notification.reply",
            "kdeconnect.sms.request", "kdeconnect.sms.request_conversations",
            "kdeconnect.sms.request_conversation", "kdeconnect.share.request",
            "kdeconnect.battery.request", "kdeconnect.telephony.request_mute",
            "kdeconnect.contacts.request_all_uids_timestamps", "kdeconnect.contacts.request_vcards_by_uid",
            "kdeconnect.findmyphone.request"]
PREFS = ("notifications", "sms", "files")


def log(*parts):
    print(time.strftime("%H:%M:%S"), *parts, file=sys.stderr, flush=True)


def read_json(path, default):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return default


def write_json(path, value, mode=0o600):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f".{path.name}.{os.getpid()}.{threading.get_ident()}")
    tmp.write_text(json.dumps(value, ensure_ascii=False, indent=1), encoding="utf-8")
    tmp.chmod(mode)
    tmp.replace(path)


def paired_devices():
    """Téléphones associés (lu aussi par les Paramètres)."""
    devices = read_json(DEVICES, {})
    return {key: value for key, value in devices.items() if isinstance(value, dict) and value.get("cert")}


def settings():
    value = read_json(SETTINGS, {})
    return value if isinstance(value, dict) else {}


def clean_name(name, limit=32):
    """Nom affichable : les caractères refusés par KDE Connect sont retirés."""
    name = re.sub(r"[\"',;:.!?()\[\]<>\x00-\x1f]", "", str(name or "")).strip()
    return name[:limit] or "Winlux"


def safe_filename(name):
    """Nom de fichier reçu : jamais de chemin, de point initial ni de caractère de contrôle."""
    name = str(name or "").replace("\\", "/").split("/")[-1]
    name = re.sub(r"[\x00-\x1f\x7f]", "", name).strip().lstrip(".").strip()
    return name[:200] or "fichier"


def unique_path(folder, name):
    target = folder / name
    stem, suffix = os.path.splitext(name)
    index = 2
    while target.exists() or target.with_name(target.name + ".part").exists():
        target = folder / f"{stem} ({index}){suffix}"
        index += 1
    return target


def download_dir():
    chosen = settings().get("download_dir")
    if chosen and Path(chosen).is_dir():
        return Path(chosen)
    try:
        for line in (HOME / ".config/user-dirs.dirs").read_text(encoding="utf-8").splitlines():
            if line.startswith("XDG_DOWNLOAD_DIR="):
                folder = Path(line.split("=", 1)[1].strip().strip('"').replace("$HOME", str(HOME)))
                if folder.is_dir():
                    return folder
    except OSError:
        pass
    for name in ("Téléchargements", "Downloads"):
        if (HOME / name).is_dir():
            return HOME / name
    folder = HOME / "Téléchargements"
    folder.mkdir(parents=True, exist_ok=True)
    return folder


def network_profile():
    """Profil du pare-feu Winlux (domain, private, public) ; privé par défaut."""
    try:
        for line in FIREWALL_CONF.read_text(encoding="utf-8").splitlines():
            if line.startswith("profile="):
                return line[8:].strip() or "private"
    except OSError:
        pass
    return "private"


def firewall_open():
    try:
        text = FIREWALL_CONF.read_text(encoding="utf-8")
    except OSError:
        return False
    return all(f"rule={rule}\t" in text for rule in FIREWALL_RULES)


def firewall(on):
    """Règles « Téléphone connecté » du pare-feu (réseaux privés), via le relais root."""
    if os.environ.get("WINLUX_PHONE_NO_FIREWALL") == "1" or firewall_open() == on:
        return
    tool = shutil.which("winlux-control")
    if tool:
        env = dict(os.environ, WINLUX_CONTROL_NO_STATUS="1")
        try:
            subprocess.run([tool, "phone-firewall-" + ("on" if on else "off")], env=env,
                           stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=60)
        except (OSError, subprocess.SubprocessError):
            pass


# ── Certificats (DER minimal, sans bibliothèque externe) ───────────────────

def _der(data, pos):
    tag = data[pos]
    length = data[pos + 1]
    pos += 2
    if length & 0x80:
        count = length & 0x7F
        length = int.from_bytes(data[pos:pos + count], "big")
        pos += count
    return tag, pos, pos + length


def certificate_parts(der):
    """(CN du sujet, SubjectPublicKeyInfo DER) d'un certificat X.509."""
    _, start, _ = _der(der, 0)
    _, tbs_start, tbs_end = _der(der, start)
    items, pos = [], tbs_start
    while pos < tbs_end:
        tag, _, end = _der(der, pos)
        items.append((tag, pos, end))
        pos = end
    if items and items[0][0] == 0xA0:
        items = items[1:]
    subject = der[items[4][1]:items[4][2]]
    spki = der[items[5][1]:items[5][2]]
    common_name = ""
    marker = subject.find(b"\x06\x03\x55\x04\x03")
    if marker >= 0:
        _, value_start, value_end = _der(subject, marker + 5)
        common_name = subject[value_start:value_end].decode("utf-8", "replace")
    return common_name, spki


def _java_compare(a, b):
    """Arrays.compare(byte[]) de Java (octets signés), comme l'application Android."""
    for x, y in zip(a, b):
        x, y = (x - 256 if x > 127 else x), (y - 256 if y > 127 else y)
        if x != y:
            return -1 if x < y else 1
    return (len(a) > len(b)) - (len(a) < len(b))


def verification_key(own_der, peer_der, timestamp):
    """Code à 8 caractères affiché des deux côtés pendant l'appairage."""
    a, b = certificate_parts(own_der)[1], certificate_parts(peer_der)[1]
    if _java_compare(a, b) < 0:
        a, b = b, a
    digest = hashlib.sha256(a + b + str(int(timestamp)).encode()).hexdigest()
    return digest[:8].upper()


def pem_to_der(text):
    body = "".join(line for line in text.splitlines() if line and not line.startswith("-----"))
    return base64.b64decode(body)


def ensure_identity():
    """Identifiant et certificat auto-signé de ce PC (créés une fois)."""
    CONFIG.mkdir(parents=True, exist_ok=True)
    CONFIG.chmod(0o700)
    id_file = CONFIG / "id"
    device_id = id_file.read_text().strip() if id_file.exists() else ""
    if not DEVICE_ID.match(device_id):
        device_id = uuid.uuid4().hex
        id_file.write_text(device_id + "\n")
    if not CERT.exists() or not KEY.exists():
        openssl = shutil.which("openssl")
        if not openssl:
            raise RuntimeError("openssl est introuvable : certificat impossible")
        subprocess.run([openssl, "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-sha256",
                        "-days", "3650", "-subj", f"/O=Winlux/OU=Winlux/CN={device_id}",
                        "-keyout", str(KEY), "-out", str(CERT)],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        KEY.chmod(0o600)
    return device_id


def device_name():
    name = settings().get("name") or socket.gethostname()
    return clean_name(name)


def device_type():
    try:
        if any(p.name.startswith("BAT") for p in Path("/sys/class/power_supply").iterdir()):
            return "laptop"
    except OSError:
        pass
    return "desktop"


def make_packet(kind, body, payload_size=None, port=None):
    packet = {"id": int(time.time() * 1000), "type": kind, "body": body}
    if payload_size is not None:
        packet["payloadSize"] = payload_size
        packet["payloadTransferInfo"] = {"port": port}
    return (json.dumps(packet, ensure_ascii=False) + "\n").encode("utf-8")


def client_context():
    """Winlux est client TLS : certificat présenté, celui du téléphone vérifié par épinglage."""
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.load_cert_chain(str(CERT), str(KEY))
    return context


def server_context():
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.verify_mode = ssl.CERT_NONE
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.load_cert_chain(str(CERT), str(KEY))
    return context


def read_plain_line(sock, limit=65536):
    """Identité en clair, avant TLS : lue octet par octet pour ne rien consommer de TLS."""
    data = b""
    while not data.endswith(b"\n"):
        chunk = sock.recv(1)
        if not chunk:
            raise ConnectionError("connexion fermée")
        data += chunk
        if len(data) > limit:
            raise ValueError("identité trop longue")
    return data


# ── Notifications du bureau ────────────────────────────────────────────────

class Desktop:
    """Notifications Winlux (org.freedesktop.Notifications) et leurs actions."""

    def __init__(self, daemon):
        self.daemon = daemon
        self.bus = None
        self.actions = {}
        self.Gio = self.GLib = None
        if os.environ.get("WINLUX_PHONE_NO_DBUS") == "1":
            return
        try:
            from gi.repository import Gio, GLib
            self.Gio, self.GLib = Gio, GLib
            self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
            self.bus.signal_subscribe("org.freedesktop.Notifications", "org.freedesktop.Notifications",
                                      "ActionInvoked", "/org/freedesktop/Notifications", None,
                                      Gio.DBusSignalFlags.NONE, self.invoked)
        except Exception as error:
            log("bus de session indisponible :", error)
            self.bus = None

    def notify(self, title, body, actions=(), replace=0, sound=True, handler=None):
        """Affiche une notification ; renvoie son numéro (0 sans bureau)."""
        log("notification :", title, "|", body[:120])
        if not self.bus:
            return 0
        flat = []
        for key, label in actions:
            flat += [key, label]
        hints = {} if sound else {"suppress-sound": self.GLib.Variant("b", True)}
        try:
            result = self.bus.call_sync(
                "org.freedesktop.Notifications", "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications", "Notify",
                self.GLib.Variant("(susssasa{sv}i)", (_t("Téléphone connecté"), replace, "phone",
                                                      title[:1024], body[:8192], flat, hints, -1)),
                None, self.Gio.DBusCallFlags.NONE, 5000, None)
            identity = result.unpack()[0]
            if handler:
                self.actions[identity] = handler
            return identity
        except Exception as error:
            log("notification impossible :", error)
            return 0

    def close(self, identity):
        self.actions.pop(identity, None)
        if self.bus and identity:
            try:
                self.bus.call_sync("org.freedesktop.Notifications", "/org/freedesktop/Notifications",
                                   "org.freedesktop.Notifications", "CloseNotification",
                                   self.GLib.Variant("(u)", (identity,)), None,
                                   self.Gio.DBusCallFlags.NONE, 5000, None)
            except Exception:
                pass

    def invoked(self, _connection, _sender, _path, _interface, _signal, params):
        identity, action = params.unpack()
        handler = self.actions.get(identity)
        if handler:
            threading.Thread(target=handler, args=(action,), daemon=True).start()


def spawn(args):
    if os.environ.get("WINLUX_PHONE_NO_GUI") == "1" or not os.environ.get("DISPLAY"):
        return
    try:
        subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, start_new_session=True)
    except OSError:
        pass


def open_path(path):
    for tool in ("winlux-open", "xdg-open"):
        if shutil.which(tool):
            spawn([tool, str(path)])
            return


# ── Liaison avec un téléphone ──────────────────────────────────────────────

class Link:
    def __init__(self, daemon, tls, identity, address, cert_der):
        self.daemon = daemon
        self.tls = tls
        self.identity = identity
        self.id = identity["deviceId"]
        self.name = clean_name(identity.get("deviceName"), 64)
        self.type = str(identity.get("deviceType", "phone"))
        self.address = address
        self.cert_der = cert_der
        self.lock = threading.Lock()
        self.alive = True

    def send(self, kind, body, payload_size=None, port=None):
        data = make_packet(kind, body, payload_size, port)
        try:
            with self.lock:
                self.tls.sendall(data)
            return True
        except (OSError, ValueError):
            self.close()
            return False

    def close(self):
        if self.alive:
            self.alive = False
            try:
                self.tls.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            try:
                self.tls.close()
            except OSError:
                pass

    def run(self):
        reader = self.tls.makefile("rb")
        try:
            while self.alive:
                line = reader.readline(MAX_LINE)
                if not line:
                    break
                if not line.endswith(b"\n"):
                    log(self.name, ": paquet trop long, liaison fermée")
                    break
                try:
                    packet = json.loads(line)
                except ValueError:
                    continue
                if isinstance(packet, dict) and isinstance(packet.get("body", {}), dict):
                    try:
                        self.daemon.handle(self, packet)
                    except Exception as error:
                        log(self.name, ": paquet", packet.get("type"), "en erreur :", repr(error))
        except (OSError, ValueError):
            pass
        finally:
            self.close()
            self.daemon.unregister(self)


class Daemon:
    def __init__(self):
        self.id = ensure_identity()
        self.own_der = pem_to_der(CERT.read_text())
        self.lock = threading.RLock()
        self.links = {}
        self.devices = paired_devices()
        self.pending = {}     # demandes reçues : id -> {timestamp, key, notification}
        self.requested = {}   # demandes envoyées : id -> timestamp
        self.battery = {}
        self.calls = {}
        self.last_seen = {}
        self.contacts_uids = {}
        self.running = True
        self.desktop = Desktop(self)
        self.public = network_profile() == "public"
        RUNTIME.mkdir(parents=True, exist_ok=True)
        RUNTIME.chmod(0o700)

    # ── identité et état ──

    def identity(self):
        return {"deviceId": self.id, "deviceName": device_name(), "deviceType": device_type(),
                "protocolVersion": PROTOCOL_VERSION, "incomingCapabilities": INCOMING,
                "outgoingCapabilities": OUTGOING, "tcpPort": self.tcp_port}

    def device_prefs(self, device_id):
        prefs = self.devices.get(device_id, {}).get("prefs", {})
        return {key: prefs.get(key, True) is not False for key in PREFS}

    def save_devices(self):
        write_json(DEVICES, self.devices)

    def publish(self):
        with self.lock:
            devices = []
            known = set(self.devices) | set(self.links) | set(self.pending)
            for device_id in sorted(known):
                link = self.links.get(device_id)
                saved = self.devices.get(device_id, {})
                pending = self.pending.get(device_id)
                devices.append({
                    "id": device_id, "name": link.name if link else saved.get("name", device_id),
                    "type": link.type if link else saved.get("type", "phone"),
                    "paired": device_id in self.devices, "connected": link is not None,
                    "battery": self.battery.get(device_id), "prefs": self.device_prefs(device_id),
                    "pair_request": {"key": pending["key"]} if pending else None,
                    "pair_sent": device_id in self.requested,
                })
            state = {"running": True, "pid": os.getpid(), "id": self.id, "name": device_name(),
                     "public_network": self.public, "devices": devices, "time": int(time.time())}
        write_json(STATUS, state)

    def data_file(self, device_id, name):
        return DATA / device_id / name

    # ── réseau ──

    def start(self):
        self.tcp = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.tcp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        for port in range(PORT, PORT + 23):
            try:
                self.tcp.bind(("0.0.0.0", port))
                break
            except OSError:
                continue
        else:
            raise RuntimeError("aucun port TCP libre")
        self.tcp_port = self.tcp.getsockname()[1]
        self.tcp.listen(8)
        self.udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.udp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.udp.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        self.udp.bind(("0.0.0.0", PORT))
        firewall(True)
        for target in (self.accept_loop, self.udp_loop, self.announce_loop, self.control_loop, self.expire_loop):
            threading.Thread(target=target, daemon=True).start()
        self.publish()
        log("Téléphone connecté :", device_name(), self.id, "TCP", self.tcp_port)

    def announce(self, address=None):
        if self.public:
            return
        data = make_packet("kdeconnect.identity", self.identity())
        targets = [(address, REMOTE_PORT)] if address else [("255.255.255.255", REMOTE_PORT)]
        if not address:
            for peer in filter(None, os.environ.get("WINLUX_PHONE_PEERS", "").split(",")):
                host, _, port = peer.rpartition(":")
                targets.append((host, int(port)))
        for target in targets:
            try:
                self.udp.sendto(data, target)
            except OSError:
                pass

    def announce_loop(self):
        # Au démarrage puis toutes les 30 s tant qu'un téléphone associé est
        # absent : le téléphone se connecte dès qu'il reçoit l'annonce.
        last = 0
        while self.running:
            self.public = network_profile() == "public"
            missing = any(device_id not in self.links for device_id in self.devices)
            if time.time() - last > 30 and (missing or not self.devices or last == 0):
                self.announce()
                last = time.time()
            time.sleep(2)

    def udp_loop(self):
        while self.running:
            try:
                data, address = self.udp.recvfrom(65536)
                packet = json.loads(data)
                body = packet.get("body", {})
            except (OSError, ValueError, AttributeError):
                continue
            if packet.get("type") != "kdeconnect.identity" or self.public:
                continue
            device_id = str(body.get("deviceId", ""))
            if device_id == self.id or not DEVICE_ID.match(device_id) or device_id in self.links:
                continue
            if time.time() - self.last_seen.get(("udp", device_id), 0) < 3:
                continue
            self.last_seen[("udp", device_id)] = time.time()
            # Réponse par notre propre annonce : le téléphone ouvre la liaison.
            self.announce(address[0])

    def accept_loop(self):
        while self.running:
            try:
                connection, address = self.tcp.accept()
            except OSError:
                continue
            threading.Thread(target=self.incoming, args=(connection, address), daemon=True).start()

    def incoming(self, connection, address):
        try:
            connection.settimeout(15)
            line = read_plain_line(connection)
            packet = json.loads(line)
            identity = packet.get("body", {})
            device_id = str(identity.get("deviceId", ""))
            if packet.get("type") != "kdeconnect.identity" or not DEVICE_ID.match(device_id) or device_id == self.id:
                raise ValueError("identité invalide")
            if self.public:
                raise ValueError("réseau public")
            tls = client_context().wrap_socket(connection, server_side=False)
            cert_der = tls.getpeercert(binary_form=True)
            if not cert_der:
                raise ValueError("pas de certificat")
            version = int(identity.get("protocolVersion", 7) or 7)
            if version >= 8:
                tls.sendall(make_packet("kdeconnect.identity", self.identity()))
                reader = tls.makefile("rb")
                second = json.loads(reader.readline(65536))
                if second.get("type") != "kdeconnect.identity" or second.get("body", {}).get("deviceId") != device_id:
                    raise ValueError("identité TLS différente")
                identity = second["body"]
                common_name = certificate_parts(cert_der)[0]
                if common_name and common_name != device_id:
                    raise ValueError("certificat d'un autre appareil")
            tls.settimeout(None)
            saved = self.devices.get(device_id)
            if saved and base64.b64decode(saved["cert"]) != cert_der:
                log("certificat inattendu pour", device_id, ": liaison refusée")
                tls.close()
                return
            link = Link(self, tls, identity, address[0], cert_der)
            self.register(link)
            link.run()
        except Exception as error:
            log("liaison refusée depuis", address[0], ":", repr(error))
            try:
                connection.close()
            except OSError:
                pass

    def register(self, link):
        with self.lock:
            old = self.links.get(link.id)
            self.links[link.id] = link
            if link.id in self.devices:
                saved = self.devices[link.id]
                if saved.get("name") != link.name or saved.get("type") != link.type:
                    saved.update(name=link.name, type=link.type)
                    self.save_devices()
        if old and old is not link:
            old.close()
        log("connecté :", link.name, link.address)
        if link.id in self.devices:
            self.on_connected(link)
        self.publish()

    def unregister(self, link):
        with self.lock:
            if self.links.get(link.id) is link:
                del self.links[link.id]
                self.pending.pop(link.id, None)
                self.requested.pop(link.id, None)
        log("déconnecté :", link.name)
        self.publish()

    def on_connected(self, link):
        prefs = self.device_prefs(link.id)
        link.send("kdeconnect.battery.request", {"request": True})
        if prefs["notifications"]:
            link.send("kdeconnect.notification.request", {"request": True})
        if prefs["sms"]:
            link.send("kdeconnect.sms.request_conversations", {})
            link.send("kdeconnect.contacts.request_all_uids_timestamps", {})

    # ── appairage ──

    def pair(self, device_id):
        link = self.links.get(device_id)
        if not link:
            raise ValueError("téléphone non connecté")
        if device_id in self.pending:
            return self.accept(device_id)
        timestamp = int(time.time())
        self.requested[device_id] = timestamp
        link.send("kdeconnect.pair", {"pair": True, "timestamp": timestamp})
        self.publish()
        return {"key": verification_key(self.own_der, link.cert_der, timestamp)}

    def accept(self, device_id):
        request = self.pending.pop(device_id, None)
        link = self.links.get(device_id)
        if not request or not link:
            raise ValueError("aucune demande d’appairage en cours")
        self.desktop.close(request.get("notification", 0))
        link.send("kdeconnect.pair", {"pair": True})
        self.paired(link)
        return {}

    def reject(self, device_id):
        request = self.pending.pop(device_id, None)
        link = self.links.get(device_id)
        if request:
            self.desktop.close(request.get("notification", 0))
        if link:
            link.send("kdeconnect.pair", {"pair": False})
        self.publish()
        return {}

    def paired(self, link):
        with self.lock:
            self.requested.pop(link.id, None)
            self.devices[link.id] = {"name": link.name, "type": link.type,
                                     "cert": base64.b64encode(link.cert_der).decode(),
                                     "paired_at": int(time.time()), "prefs": {key: True for key in PREFS}}
            self.save_devices()
        self.desktop.notify(_t("Téléphone associé"), _t("{name} est maintenant relié à ce PC.").format(name=link.name))
        self.on_connected(link)
        self.publish()

    def unpair(self, device_id, tell=True):
        with self.lock:
            saved = self.devices.pop(device_id, None)
            self.save_devices()
            link = self.links.get(device_id)
        if link and tell:
            link.send("kdeconnect.pair", {"pair": False})
        name = (link.name if link else (saved or {}).get("name", device_id))
        if saved:
            self.desktop.notify(_t("Téléphone dissocié"), _t("{name} n’est plus relié à ce PC.").format(name=name))
        shutil.rmtree(DATA / device_id, ignore_errors=True)
        self.publish()
        return {}

    def handle_pair(self, link, body):
        if body.get("pair") is True:
            if link.id in self.requested:
                self.paired(link)
            elif link.id in self.devices:
                link.send("kdeconnect.pair", {"pair": True})
            else:
                timestamp = body.get("timestamp")
                if timestamp is not None and abs(time.time() - int(timestamp)) > 1800:
                    log("demande d'appairage refusée : horloges trop différentes")
                    link.send("kdeconnect.pair", {"pair": False})
                    return
                key = verification_key(self.own_der, link.cert_der, timestamp) if timestamp is not None else ""
                request = {"time": time.time(), "key": key}
                self.pending[link.id] = request
                request["notification"] = self.desktop.notify(
                    _t("{name} veut s’associer à ce PC").format(name=link.name),
                    _t("Vérifiez que le téléphone affiche le même code : {key}").format(key=key or "—"),
                    [("accept", _t("Accepter")), ("reject", _t("Refuser"))],
                    handler=lambda action, d=link.id: (self.accept if action == "accept" else self.reject)(d)
                    if action in ("accept", "reject") else None)
                spawn([sys.executable, os.path.abspath(__file__), "pair-dialog", link.id])
                self.publish()
        else:
            if link.id in self.devices:
                self.unpair(link.id, tell=False)
            pending = self.pending.pop(link.id, None)
            if pending:
                self.desktop.close(pending.get("notification", 0))
            self.requested.pop(link.id, None)
            self.publish()

    def expire_loop(self):
        while self.running:
            time.sleep(1)
            now = time.time()
            changed = False
            for device_id, request in list(self.pending.items()):
                if now - request["time"] > PAIR_TIMEOUT:
                    self.pending.pop(device_id, None)
                    self.desktop.close(request.get("notification", 0))
                    changed = True
            for device_id, stamp in list(self.requested.items()):
                if now - stamp > PAIR_TIMEOUT:
                    self.requested.pop(device_id, None)
                    changed = True
            if changed:
                self.publish()

    # ── paquets reçus ──

    def handle(self, link, packet):
        kind, body = packet.get("type", ""), packet.get("body", {})
        if kind == "kdeconnect.pair":
            return self.handle_pair(link, body)
        if link.id not in self.devices:
            # Jamais rien d'un téléphone non associé : on le lui rappelle.
            if kind != "kdeconnect.identity":
                link.send("kdeconnect.pair", {"pair": False})
            return
        prefs = self.device_prefs(link.id)
        handlers = {
            "kdeconnect.notification": self.on_notification,
            "kdeconnect.sms.messages": self.on_sms,
            "kdeconnect.share.request": self.on_share,
            "kdeconnect.battery": self.on_battery,
            "kdeconnect.telephony": self.on_call,
            "kdeconnect.ping": self.on_ping,
            "kdeconnect.findmyphone.request": self.on_find,
            "kdeconnect.contacts.response_uids_timestamps": self.on_contact_uids,
            "kdeconnect.contacts.response_vcards": self.on_vcards,
        }
        needs = {"kdeconnect.notification": "notifications", "kdeconnect.sms.messages": "sms",
                 "kdeconnect.share.request": "files", "kdeconnect.contacts.response_vcards": "sms",
                 "kdeconnect.contacts.response_uids_timestamps": "sms"}
        if kind in needs and not prefs[needs[kind]]:
            return
        handler = handlers.get(kind)
        if handler:
            handler(link, packet, body)

    def on_notification(self, link, _packet, body):
        path = self.data_file(link.id, "notifications.json")
        with self.lock:
            items = [item for item in read_json(path, []) if item.get("id") != body.get("id")]
            if body.get("isCancel"):
                for item in read_json(path, []):
                    if item.get("id") == body.get("id"):
                        self.desktop.close(item.get("local", 0))
                write_json(path, items)
                self.publish()
                return
            item = {"id": str(body.get("id", "")), "app": str(body.get("appName", ""))[:100],
                    "title": str(body.get("title", "") or body.get("ticker", ""))[:500],
                    "text": str(body.get("text", ""))[:4000],
                    "time": int(body.get("time", 0) or time.time() * 1000),
                    "reply": str(body.get("requestReplyId", "") or ""),
                    "clearable": bool(body.get("isClearable", True))}
            if not body.get("silent"):
                title = f"{item['app']} · {item['title']}" if item["app"] and item["title"] != item["app"] else (item["title"] or item["app"])
                actions = []
                if item["reply"]:
                    actions.append(("reply", _t("Répondre")))
                if item["clearable"]:
                    actions.append(("dismiss", _t("Ignorer")))
                item["local"] = self.desktop.notify(
                    title, item["text"], actions,
                    handler=lambda action, d=link.id, i=item: self.notification_action(d, i, action))
            items.append(item)
            write_json(path, items[-100:])
        self.publish()

    def notification_action(self, device_id, item, action):
        if action == "reply" and item.get("reply"):
            spawn([sys.executable, os.path.abspath(__file__), "reply", device_id, item["reply"],
                   item.get("title", "")])
        elif action == "dismiss":
            self.dismiss(device_id, item["id"])

    def dismiss(self, device_id, notification_id):
        link = self.links.get(device_id)
        if link:
            link.send("kdeconnect.notification.request", {"cancel": notification_id})
        path = self.data_file(device_id, "notifications.json")
        with self.lock:
            write_json(path, [item for item in read_json(path, []) if item.get("id") != notification_id])
        self.publish()
        return {}

    def on_sms(self, link, _packet, body):
        path = self.data_file(link.id, "sms.json")
        with self.lock:
            threads = read_json(path, {})
            for message in body.get("messages", []):
                if not isinstance(message, dict):
                    continue
                thread = str(message.get("thread_id", ""))
                if not thread:
                    continue
                entry = {"id": message.get("_id"), "body": str(message.get("body", ""))[:5000],
                         "addresses": [str(a.get("address", "")) for a in message.get("addresses", []) if isinstance(a, dict)],
                         "date": int(message.get("date", 0) or 0), "type": int(message.get("type", 1) or 1),
                         "read": int(message.get("read", 1) or 0)}
                current = [m for m in threads.get(thread, []) if m.get("id") != entry["id"] or entry["id"] is None]
                current.append(entry)
                current.sort(key=lambda m: m.get("date", 0))
                threads[thread] = current[-500:]
            write_json(path, threads)
        self.publish()

    def on_contact_uids(self, link, _packet, body):
        uids = [str(uid) for uid in body.get("uids", []) if uid][:5000]
        if uids:
            link.send("kdeconnect.contacts.request_vcards_by_uid", {"uids": uids})

    def on_vcards(self, link, _packet, body):
        path = self.data_file(link.id, "contacts.json")
        with self.lock:
            contacts = read_json(path, {})
            for uid in body.get("uids", []):
                card = body.get(str(uid))
                if not isinstance(card, str):
                    continue
                name, numbers = "", []
                for line in card.replace("\r\n ", "").splitlines():
                    key, _, value = line.partition(":")
                    key = key.split(";")[0].upper()
                    if key == "FN":
                        name = value.strip()
                    elif key == "TEL" and value.strip():
                        numbers.append(value.strip())
                for number in numbers:
                    digits = re.sub(r"\D", "", number)[-9:]
                    if digits and name:
                        contacts[digits] = name[:100]
            write_json(path, contacts)
        self.publish()

    def on_share(self, link, packet, body):
        if "filename" in body and packet.get("payloadTransferInfo"):
            threading.Thread(target=self.download, args=(link, packet, body), daemon=True).start()
        elif isinstance(body.get("url"), str) and re.match(r"^https?://", body["url"]):
            url = body["url"][:4000]
            self.record_received(link.id, {"kind": "url", "text": url})
            self.desktop.notify(_t("Lien reçu de {name}").format(name=link.name), url,
                                [("open", _t("Ouvrir"))],
                                handler=lambda action, u=url: open_path(u) if action == "open" else None)
        elif isinstance(body.get("text"), str):
            text = body["text"][:20000]
            self.record_received(link.id, {"kind": "text", "text": text})
            self.desktop.notify(_t("Texte reçu de {name}").format(name=link.name), text)

    def record_received(self, device_id, entry):
        path = self.data_file(device_id, "received.json")
        entry["time"] = int(time.time())
        with self.lock:
            items = read_json(path, [])
            items.append(entry)
            write_json(path, items[-200:])
        self.publish()

    def download(self, link, packet, body):
        folder = download_dir()
        target = unique_path(folder, safe_filename(body.get("filename")))
        partial = target.with_name(target.name + ".part")
        size = int(packet.get("payloadSize", -1) or -1)
        try:
            if size > 0 and shutil.disk_usage(folder).free < size + 64 * 1024 * 1024:
                raise OSError("espace disque insuffisant")
            port = int(packet["payloadTransferInfo"]["port"])
            raw = socket.create_connection((link.address, port), timeout=20)
            tls = client_context().wrap_socket(raw, server_side=False)
            if tls.getpeercert(binary_form=True) != link.cert_der:
                tls.close()
                raise OSError("certificat différent pour le transfert")
            received = 0
            with open(partial, "wb") as out:
                os.fchmod(out.fileno(), 0o644)
                while size < 0 or received < size:
                    chunk = tls.recv(min(1 << 20, size - received) if size > 0 else 1 << 20)
                    if not chunk:
                        break
                    out.write(chunk)
                    received += len(chunk)
            tls.close()
            if size > 0 and received != size:
                raise OSError(f"transfert incomplet ({received}/{size})")
            stamp = body.get("lastModified")
            if stamp:
                try:
                    os.utime(partial, (time.time(), int(stamp) / 1000))
                except (OSError, ValueError, OverflowError):
                    pass
            partial.replace(target)
        except (OSError, ValueError, KeyError, ssl.SSLError) as error:
            log("réception de", target.name, "impossible :", repr(error))
            partial.unlink(missing_ok=True)
            return
        self.record_received(link.id, {"kind": "file", "path": str(target), "name": target.name, "size": received})
        self.desktop.notify(_t("Fichier reçu de {name}").format(name=link.name),
                            target.name + "\n" + _t("Enregistré dans {folder}").format(folder=folder),
                            [("open", _t("Ouvrir le dossier"))],
                            handler=lambda action, f=folder: open_path(f) if action == "open" else None)

    def upload(self, device_id, path):
        link = self.links.get(device_id)
        path = Path(path)
        if not link or device_id not in self.devices:
            raise ValueError("téléphone non connecté")
        if not path.is_file():
            raise ValueError(f"fichier introuvable : {path}")
        server = None
        for port in PAYLOAD_PORTS:
            candidate = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            candidate.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            try:
                candidate.bind(("0.0.0.0", port))
                server = candidate
                break
            except OSError:
                candidate.close()
        if not server:
            raise ValueError("aucun port de transfert libre")
        size = path.stat().st_size
        server.listen(1)
        threading.Thread(target=self.serve_upload, args=(link, server, path, size), daemon=True).start()
        link.send("kdeconnect.share.request",
                  {"filename": path.name, "lastModified": int(path.stat().st_mtime * 1000),
                   "numberOfFiles": 1, "totalPayloadSize": size},
                  payload_size=size, port=server.getsockname()[1])
        return {"port": server.getsockname()[1]}

    def serve_upload(self, link, server, path, size):
        deadline = time.time() + 120
        sent = 0
        try:
            while time.time() < deadline:
                ready, _, _ = select.select([server], [], [], max(0.1, deadline - time.time()))
                if not ready:
                    continue
                connection, address = server.accept()
                if address[0] != link.address:
                    connection.close()
                    continue
                connection.settimeout(60)
                tls = server_context().wrap_socket(connection, server_side=True)
                with open(path, "rb") as source:
                    while True:
                        chunk = source.read(1 << 20)
                        if not chunk:
                            break
                        tls.sendall(chunk)
                        sent += len(chunk)
                try:
                    tls.unwrap()
                except (OSError, ssl.SSLError):
                    pass
                tls.close()
                break
        except (OSError, ssl.SSLError) as error:
            log("envoi de", path.name, "interrompu :", repr(error))
        finally:
            server.close()
        if sent == size:
            self.desktop.notify(_t("Fichier envoyé"), _t("{file} est sur {name}.").format(file=path.name, name=link.name),
                                sound=False)
        else:
            self.desktop.notify(_t("Envoi de fichier impossible"),
                                _t("Le téléphone ne s’est pas connecté pour recevoir {file}.").format(file=path.name))

    def on_battery(self, link, _packet, body):
        level = body.get("currentCharge")
        if not isinstance(level, int) or level < 0:
            return
        self.battery[link.id] = {"level": level, "charging": bool(body.get("isCharging"))}
        if body.get("thresholdEvent") == 1:
            self.desktop.notify(_t("Batterie faible"), _t("{name} : {level} % restants").format(name=link.name, level=level))
        self.publish()

    def on_call(self, link, _packet, body):
        event = body.get("event")
        who = str(body.get("contactName") or body.get("phoneNumber") or "")
        key = (link.id, str(body.get("phoneNumber", "")))
        if body.get("isCancel"):
            self.desktop.close(self.calls.pop(key, 0))
            return
        if event == "ringing":
            self.calls[key] = self.desktop.notify(
                _t("Appel entrant"), who, [("mute", _t("Couper la sonnerie"))],
                replace=self.calls.get(key, 0),
                handler=lambda action, l=link: l.send("kdeconnect.telephony.request_mute", {}) if action == "mute" else None)
        elif event == "missedCall":
            self.desktop.close(self.calls.pop(key, 0))
            self.desktop.notify(_t("Appel manqué"), who)
        elif event == "talking":
            self.desktop.close(self.calls.pop(key, 0))

    def on_ping(self, link, _packet, body):
        self.desktop.notify(_t("Ping de {name}").format(name=link.name), str(body.get("message", ""))[:500])

    def on_find(self, link, _packet, _body):
        self.desktop.notify(_t("{name} cherche ce PC").format(name=link.name), "")
        for _ in range(3):
            for player in ("pw-play", "paplay", "aplay"):
                sound = Path(os.environ.get("WINLUX_SOUNDS_DIR", "/usr/share/winlux/sounds")) / "notification.wav"
                if shutil.which(player) and sound.is_file():
                    subprocess.run([player, str(sound)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
                    break

    # ── commandes (socket de contrôle) ──

    def command(self, request):
        cmd = request.get("cmd")
        device = str(request.get("device", ""))
        if cmd == "status":
            self.publish()
            return read_json(STATUS, {})
        if cmd == "announce":
            self.announce()
            return {}
        if cmd == "pair":
            return self.pair(device)
        if cmd == "accept":
            return self.accept(device)
        if cmd == "reject":
            return self.reject(device)
        if cmd == "unpair":
            return self.unpair(device)
        if cmd == "stop":
            self.running = False
            return {}
        link = self.links.get(device)
        if cmd == "set":
            key, value = request.get("key"), bool(request.get("value"))
            if device not in self.devices or key not in PREFS:
                raise ValueError("réglage inconnu")
            with self.lock:
                self.devices[device].setdefault("prefs", {})[key] = value
                self.save_devices()
            if not value:
                # Fonction coupée : ce qui en venait est effacé du PC.
                for name in {"notifications": ("notifications.json",), "sms": ("sms.json", "contacts.json"),
                             "files": ("received.json",)}[key]:
                    (DATA / device / name).unlink(missing_ok=True)
            elif link:
                self.on_connected(link)
            self.publish()
            return {}
        if cmd == "dismiss":
            return self.dismiss(device, str(request.get("notification", "")))
        if not link or device not in self.devices:
            raise ValueError("téléphone non connecté")
        if cmd == "ring":
            link.send("kdeconnect.findmyphone.request", {})
            return {}
        if cmd == "send":
            return self.upload(device, request.get("path", ""))
        if cmd == "sms":
            addresses = [str(a) for a in request.get("addresses", []) if str(a).strip()]
            text = str(request.get("text", ""))
            if not addresses or not text.strip():
                raise ValueError("numéro ou message manquant")
            link.send("kdeconnect.sms.request", {"version": 2, "messageBody": text, "phoneNumber": addresses[0],
                                                 "addresses": [{"address": a} for a in addresses]})
            return {}
        if cmd == "thread":
            link.send("kdeconnect.sms.request_conversation",
                      {"threadID": int(request.get("thread", 0)), "numberToRequest": 100})
            return {}
        if cmd == "reply":
            link.send("kdeconnect.notification.reply",
                      {"requestReplyId": str(request.get("reply", "")), "message": str(request.get("text", ""))})
            return {}
        if cmd == "refresh":
            self.on_connected(link)
            return {}
        raise ValueError(f"commande inconnue : {cmd}")

    def control_loop(self):
        SOCKET.unlink(missing_ok=True)
        server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        server.bind(str(SOCKET))
        SOCKET.chmod(0o600)
        server.listen(8)
        while self.running:
            try:
                connection, _ = server.accept()
            except OSError:
                continue
            threading.Thread(target=self.control_client, args=(connection,), daemon=True).start()

    def control_client(self, connection):
        with connection:
            try:
                connection.settimeout(10)
                # Même compte seulement (le dossier est déjà en 0700).
                creds = connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12)
                if int.from_bytes(creds[4:8], sys.byteorder) not in (os.getuid(), 0):
                    return
                request = json.loads(connection.makefile("rb").readline(1 << 20))
                answer = {"ok": True}
                answer.update(self.command(request) or {})
            except Exception as error:
                answer = {"ok": False, "error": str(error)}
            try:
                connection.sendall((json.dumps(answer, ensure_ascii=False) + "\n").encode())
            except OSError:
                pass

    def stop(self):
        self.running = False
        for link in list(self.links.values()):
            link.close()
        for sock in (self.tcp, self.udp):
            try:
                sock.close()
            except OSError:
                pass
        SOCKET.unlink(missing_ok=True)
        state = read_json(STATUS, {})
        state["running"] = False
        write_json(STATUS, state)


def serve(if_paired=False):
    if settings().get("enabled") is False or (if_paired and not paired_devices()):
        return 0
    if request({"cmd": "status"}).get("ok"):
        return 0  # déjà lancé pour ce compte
    daemon = Daemon()
    daemon.start()
    import signal
    signal.signal(signal.SIGTERM, lambda *_: setattr(daemon, "running", False))
    try:
        if daemon.desktop.GLib:
            loop = daemon.desktop.GLib.MainLoop()
            daemon.desktop.GLib.timeout_add(500, lambda: daemon.running or loop.quit())
            loop.run()
        else:
            while daemon.running:
                time.sleep(0.5)
    except KeyboardInterrupt:
        pass
    daemon.stop()
    return 0


# ── Client (CLI, application, Paramètres) ──────────────────────────────────

def request(message, timeout=10):
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(timeout)
            client.connect(str(SOCKET))
            client.sendall((json.dumps(message) + "\n").encode())
            return json.loads(client.makefile("rb").readline(1 << 24) or b"{}")
    except (OSError, ValueError):
        return {"ok": False, "error": _t("Le service Téléphone connecté ne répond pas.")}


def ensure_service():
    if request({"cmd": "status"}, timeout=2).get("ok"):
        return True
    try:
        subprocess.Popen([sys.executable, os.path.abspath(__file__), "serve"], stdin=subprocess.DEVNULL,
                         stdout=subprocess.DEVNULL, stderr=open(RUNTIME_LOG(), "ab"), start_new_session=True)
    except OSError:
        return False
    for _ in range(50):
        if request({"cmd": "status"}, timeout=1).get("ok"):
            return True
        time.sleep(0.1)
    return False


def RUNTIME_LOG():
    RUNTIME.mkdir(parents=True, exist_ok=True)
    return RUNTIME / "service.log"


def contact_name(contacts, address):
    digits = re.sub(r"\D", "", address)[-9:]
    return contacts.get(digits, address) if digits else address


# ── Fenêtres GTK ───────────────────────────────────────────────────────────

CSS = b"""
window { background: #181b24; color: #ecedf5; }
.side { background: #202430; }
.heading { font-size: 22px; font-weight: bold; }
.subtitle { color: #b6bdd3; }
.card { background: #242a38; border-radius: 8px; padding: 14px; }
.code { font-size: 28px; font-weight: bold; letter-spacing: 4px; }
.mine { background: #423d66; border-radius: 12px; padding: 8px 12px; }
.theirs { background: #2c3344; border-radius: 12px; padding: 8px 12px; }
list, list row { background: #242a38; color: #ecedf5; }
list row:selected { background: #423d66; }
entry { background: #171b25; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; padding: 8px; }
button { background: #30384b; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; padding: 8px 14px; box-shadow: none; }
button:hover { background: #414b64; }
button.suggested-action { background: #5b52a3; }
stackswitcher button:checked { background: #423d66; border-color: #b69cff; }
"""


def _gtk():
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk, Gdk, GLib
    provider = Gtk.CssProvider()
    provider.load_from_data(CSS)
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), provider,
                                             Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
    return Gtk, GLib


def label(Gtk, text, style=None, **kwargs):
    widget = Gtk.Label(label=text, xalign=0, wrap=True, **kwargs)
    if style:
        widget.get_style_context().add_class(style)
    return widget


def pair_dialog(device_id):
    Gtk, GLib = _gtk()
    state = read_json(STATUS, {})
    device = next((d for d in state.get("devices", []) if d["id"] == device_id), None)
    if not device or not device.get("pair_request"):
        return 0
    window = Gtk.Window(title=_t("Téléphone connecté"))
    window.set_icon_name("phone")
    window.set_keep_above(True)
    window.set_resizable(False)
    window.set_position(Gtk.WindowPosition.CENTER)
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12, margin=24)
    window.add(box)
    box.pack_start(label(Gtk, _t("{name} veut s’associer à ce PC").format(name=device["name"]), "heading"), False, False, 0)
    key = device["pair_request"].get("key") or "—"
    box.pack_start(label(Gtk, _t("Vérifiez que le téléphone affiche le même code : {key}").format(key=""), "subtitle"), False, False, 0)
    box.pack_start(label(Gtk, key, "code"), False, False, 0)
    buttons = Gtk.Box(spacing=8, margin_top=8)
    box.pack_start(buttons, False, False, 0)
    for text, cmd, style in ((_t("Refuser"), "reject", None), (_t("Accepter"), "accept", "suggested-action")):
        button = Gtk.Button(label=text)
        if style:
            button.get_style_context().add_class(style)
        button.connect("clicked", lambda *_, c=cmd: (request({"cmd": c, "device": device_id}), Gtk.main_quit()))
        buttons.pack_end(button, False, False, 0)
    window.connect("destroy", Gtk.main_quit)
    GLib.timeout_add_seconds(PAIR_TIMEOUT, Gtk.main_quit)
    window.show_all()
    Gtk.main()
    return 0


def reply_dialog(device_id, reply_id, title=""):
    Gtk, GLib = _gtk()
    window = Gtk.Window(title=_t("Répondre"))
    window.set_icon_name("phone")
    window.set_default_size(420, -1)
    window.set_position(Gtk.WindowPosition.CENTER)
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12, margin=20)
    window.add(box)
    box.pack_start(label(Gtk, title or _t("Répondre"), "heading"), False, False, 0)
    entry = Gtk.Entry(placeholder_text=_t("Écrire une réponse"))
    box.pack_start(entry, False, False, 0)

    def send(*_):
        if entry.get_text().strip():
            request({"cmd": "reply", "device": device_id, "reply": reply_id, "text": entry.get_text()})
            Gtk.main_quit()
    entry.connect("activate", send)
    button = Gtk.Button(label=_t("Envoyer"))
    button.get_style_context().add_class("suggested-action")
    button.connect("clicked", send)
    line = Gtk.Box(spacing=8)
    line.pack_end(button, False, False, 0)
    box.pack_start(line, False, False, 0)
    window.connect("destroy", Gtk.main_quit)
    window.show_all()
    Gtk.main()
    return 0


class PhoneApp:
    """Application « Téléphone connecté » (façon Lien avec un téléphone)."""

    def __init__(self, Gtk, GLib, page=None, thread=None):
        self.Gtk, self.GLib = Gtk, GLib
        self.page = page
        self.window = Gtk.Window(title=_t("Téléphone connecté"))
        self.window.set_icon_name("phone")
        self.window.set_default_size(980, 680)
        self.window.connect("destroy", lambda *_: Gtk.main_quit())
        self.root = Gtk.Box()
        self.window.add(self.root)
        self.stamp = None
        self.thread = thread
        self.device = None
        self.view = None
        self.message = ""
        ensure_service()
        self.refresh(force=True)
        GLib.timeout_add(1000, self.refresh)
        self.window.show_all()

    def state(self):
        return read_json(STATUS, {})

    def refresh(self, force=False):
        try:
            stamp = tuple(os.stat(p).st_mtime_ns if p.exists() else 0 for p in
                          [STATUS] + ([DATA / self.device / n for n in ("notifications.json", "sms.json", "received.json", "contacts.json")]
                                      if self.device else []))
        except OSError:
            stamp = None
        if force or stamp != self.stamp:
            self.stamp = stamp
            self.build()
        return True

    def clear(self):
        for child in self.root.get_children():
            self.root.remove(child)

    def build(self):
        state = self.state()
        devices = state.get("devices", [])
        paired = [d for d in devices if d.get("paired")]
        self.clear()
        if not paired or self.view == "pair":
            self.device = None
            self.build_pairing(state, devices)
        else:
            if self.device not in [d["id"] for d in paired]:
                self.device = paired[0]["id"]
            device = next(d for d in paired if d["id"] == self.device)
            self.build_device(state, device, paired)
        self.root.show_all()

    def build_pairing(self, state, devices):
        Gtk = self.Gtk
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14, margin=32)
        self.root.pack_start(box, True, True, 0)
        box.pack_start(label(Gtk, _t("Reliez votre téléphone Android à ce PC"), "heading"), False, False, 0)
        for line in ("1. Installez l’application libre KDE Connect sur le téléphone (F-Droid ou Google Play).",
                     "2. Connectez le téléphone au même réseau Wi-Fi que ce PC (réseau privé).",
                     "3. Ouvrez KDE Connect : ce PC apparaît sous le nom « {name} ». Associez-le depuis le téléphone ou ci-dessous."):
            box.pack_start(label(Gtk, _t(line).format(name=state.get("name", "Winlux")), "subtitle"), False, False, 0)
        if state.get("public_network"):
            box.pack_start(label(Gtk, _t("Ce réseau est public : le téléphone ne peut pas trouver ce PC. Passez le réseau en privé dans Paramètres › Réseau et Internet.")), False, False, 0)
        if not state.get("running"):
            box.pack_start(label(Gtk, _t("Le service Téléphone connecté ne répond pas.")), False, False, 0)
        box.pack_start(label(Gtk, _t("Téléphones à proximité"), "heading"), False, False, 8)
        card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        card.get_style_context().add_class("card")
        box.pack_start(card, False, False, 0)
        nearby = [d for d in devices if d.get("connected") and not d.get("paired")]
        if not nearby:
            card.pack_start(label(Gtk, _t("Recherche des téléphones sur le réseau…"), "subtitle"), False, False, 0)
        for device in nearby:
            line = Gtk.Box(spacing=10)
            line.pack_start(label(Gtk, device["name"]), True, True, 0)
            request_in = device.get("pair_request")
            if request_in:
                line.pack_start(label(Gtk, request_in.get("key") or "", "code"), False, False, 0)
                for text, cmd in ((_t("Refuser"), "reject"), (_t("Accepter"), "accept")):
                    button = Gtk.Button(label=text)
                    button.connect("clicked", lambda *_, c=cmd, d=device["id"]: self.act({"cmd": c, "device": d}))
                    line.pack_start(button, False, False, 0)
            elif device.get("pair_sent"):
                line.pack_start(label(Gtk, _t("Demande envoyée : acceptez-la sur le téléphone."), "subtitle"), False, False, 0)
            else:
                button = Gtk.Button(label=_t("Associer"))
                button.get_style_context().add_class("suggested-action")
                button.connect("clicked", lambda *_, d=device["id"]: self.act({"cmd": "pair", "device": d}))
                line.pack_start(button, False, False, 0)
            card.pack_start(line, False, False, 0)
        actions = Gtk.Box(spacing=8)
        again = Gtk.Button(label=_t("Rechercher à nouveau"))
        again.connect("clicked", lambda *_: self.act({"cmd": "announce"}))
        actions.pack_start(again, False, False, 0)
        if any(d.get("paired") for d in devices):
            back = Gtk.Button(label=_t("Annuler"))
            back.connect("clicked", lambda *_: self.show_view(None))
            actions.pack_start(back, False, False, 0)
        box.pack_start(actions, False, False, 0)
        if self.message:
            box.pack_start(label(Gtk, self.message, "subtitle"), False, False, 0)

    def show_view(self, view):
        self.view = view
        self.build()

    def act(self, message):
        answer = request(message)
        self.message = "" if answer.get("ok") else answer.get("error", "")
        self.build()
        return answer

    def build_device(self, state, device, paired):
        Gtk = self.Gtk
        side = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin=0)
        side.get_style_context().add_class("side")
        side.set_size_request(250, -1)
        inner = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin=18)
        side.pack_start(inner, True, True, 0)
        self.root.pack_start(side, False, False, 0)
        if len(paired) > 1:
            combo = Gtk.ComboBoxText()
            for d in paired:
                combo.append(d["id"], d["name"])
            combo.set_active_id(device["id"])
            combo.connect("changed", lambda c: (setattr(self, "device", c.get_active_id()), self.build()))
            inner.pack_start(combo, False, False, 0)
        inner.pack_start(label(Gtk, device["name"], "heading"), False, False, 0)
        status = _t("Connecté") if device.get("connected") else _t("Hors de portée")
        battery = device.get("battery")
        if battery:
            status += " · " + _t("Batterie {level} %").format(level=battery["level"])
            if battery.get("charging"):
                status += " (" + _t("en charge") + ")"
        inner.pack_start(label(Gtk, status, "subtitle"), False, False, 0)
        for text, handler in ((_t("Faire sonner"), lambda *_: self.act({"cmd": "ring", "device": device["id"]})),
                              (_t("Envoyer un fichier…"), lambda *_: self.choose_files(device["id"])),
                              (_t("Ajouter un téléphone"), lambda *_: self.show_view("pair"))):
            button = Gtk.Button(label=text)
            if text != _t("Ajouter un téléphone"):
                button.set_sensitive(bool(device.get("connected")))
            button.connect("clicked", handler)
            inner.pack_start(button, False, False, 0)
        if self.message:
            inner.pack_start(label(Gtk, self.message, "subtitle"), False, False, 0)
        stack = Gtk.Stack()
        switcher = Gtk.StackSwitcher(stack=stack, halign=Gtk.Align.CENTER, margin=10)
        main = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        main.pack_start(switcher, False, False, 0)
        main.pack_start(stack, True, True, 0)
        self.root.pack_start(main, True, True, 0)
        prefs = device.get("prefs", {})
        if prefs.get("notifications", True):
            stack.add_titled(self.notifications_page(device), "notifications", _t("Notifications"))
        if prefs.get("sms", True):
            stack.add_titled(self.messages_page(device), "messages", _t("Messages"))
        if prefs.get("files", True):
            stack.add_titled(self.files_page(device), "files", _t("Fichiers"))
        if getattr(self, "page", None) and stack.get_child_by_name(self.page):
            stack.show_all()
            stack.set_visible_child_name(self.page)
        stack.connect("notify::visible-child-name", lambda s, _: setattr(self, "page", s.get_visible_child_name()))

    def scrolled(self, child):
        scroll = self.Gtk.ScrolledWindow(hscrollbar_policy=self.Gtk.PolicyType.NEVER)
        scroll.add(child)
        return scroll

    def notifications_page(self, device):
        Gtk = self.Gtk
        items = read_json(DATA / device["id"] / "notifications.json", [])
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin=16)
        if not items:
            box.pack_start(label(Gtk, _t("Aucune notification du téléphone."), "subtitle"), False, False, 0)
            box.pack_start(label(Gtk, _t("Sur le téléphone, autorisez KDE Connect à lire les notifications."), "subtitle"), False, False, 0)
        for item in sorted(items, key=lambda i: -int(i.get("time", 0) or 0)):
            card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
            card.get_style_context().add_class("card")
            when = time.strftime("%d/%m %H:%M", time.localtime(item.get("time", 0) / 1000))
            card.pack_start(label(Gtk, f"{item.get('app', '')} · {when}", "subtitle"), False, False, 0)
            if item.get("title"):
                card.pack_start(label(Gtk, item["title"], selectable=True), False, False, 0)
            if item.get("text"):
                card.pack_start(label(Gtk, item["text"], "subtitle", selectable=True), False, False, 0)
            line = Gtk.Box(spacing=8)
            if item.get("reply") and device.get("connected"):
                entry = Gtk.Entry(placeholder_text=_t("Écrire une réponse"))
                send = Gtk.Button(label=_t("Envoyer"))

                def reply(*_, e=entry, r=item["reply"]):
                    if e.get_text().strip():
                        self.act({"cmd": "reply", "device": device["id"], "reply": r, "text": e.get_text()})
                entry.connect("activate", reply)
                send.connect("clicked", reply)
                line.pack_start(entry, True, True, 0)
                line.pack_start(send, False, False, 0)
            if item.get("clearable", True):
                dismiss = Gtk.Button(label=_t("Ignorer"))
                dismiss.connect("clicked", lambda *_, i=item["id"]: self.act({"cmd": "dismiss", "device": device["id"], "notification": i}))
                line.pack_end(dismiss, False, False, 0)
            card.pack_start(line, False, False, 0)
            box.pack_start(card, False, False, 0)
        return self.scrolled(box)

    def messages_page(self, device):
        Gtk = self.Gtk
        threads = read_json(DATA / device["id"] / "sms.json", {})
        contacts = read_json(DATA / device["id"] / "contacts.json", {})
        paned = Gtk.Box(spacing=0)
        listing = Gtk.ListBox()
        listing.set_size_request(300, -1)
        ordered = sorted(threads.items(), key=lambda kv: -(kv[1][-1].get("date", 0) if kv[1] else 0))
        new = Gtk.Button(label=_t("Nouveau message"))
        new.connect("clicked", lambda *_: (setattr(self, "thread", "new"), self.build()))
        left = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin=10)
        left.pack_start(new, False, False, 0)
        if not ordered:
            left.pack_start(label(Gtk, _t("Aucune conversation pour l’instant. Autorisez l’accès aux SMS dans KDE Connect sur le téléphone."), "subtitle"), False, False, 0)
        for thread, messages in ordered:
            last = messages[-1] if messages else {}
            who = ", ".join(contact_name(contacts, a) for a in last.get("addresses", [])) or thread
            row = Gtk.ListBoxRow()
            row.thread = thread
            cell = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, margin=8)
            cell.pack_start(label(Gtk, who), False, False, 0)
            snippet = (_t("Vous : ") if last.get("type") == 2 else "") + last.get("body", "").replace("\n", " ")[:60]
            cell.pack_start(label(Gtk, snippet, "subtitle"), False, False, 0)
            row.add(cell)
            listing.add(row)
            if thread == self.thread:
                listing.select_row(row)
        listing.connect("row-activated", lambda _l, row: self.open_thread(device, row.thread))
        left.pack_start(self.scrolled(listing), True, True, 0)
        paned.pack_start(left, False, False, 0)
        right = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin=10)
        paned.pack_start(right, True, True, 0)
        addresses = []
        if self.thread == "new":
            number = Gtk.Entry(placeholder_text=_t("Numéro de téléphone"))
            right.pack_start(number, False, False, 0)
            right.pack_start(Gtk.Box(), True, True, 0)
        elif self.thread in threads:
            number = None
            bubbles = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6, margin=6)
            for message in threads[self.thread]:
                mine = message.get("type") == 2
                bubble = label(Gtk, message.get("body", ""), "mine" if mine else "theirs", selectable=True,
                               max_width_chars=50)
                bubble.set_halign(Gtk.Align.END if mine else Gtk.Align.START)
                bubbles.pack_start(bubble, False, False, 0)
                if not addresses:
                    addresses = message.get("addresses", [])
            scroll = self.scrolled(bubbles)
            right.pack_start(scroll, True, True, 0)
            adjustment = scroll.get_vadjustment()
            adjustment.connect("changed", lambda a: a.set_value(a.get_upper()))
        else:
            right.pack_start(label(Gtk, _t("Choisissez une conversation."), "subtitle"), True, True, 0)
            return paned
        line = Gtk.Box(spacing=8)
        entry = Gtk.Entry(placeholder_text=_t("Écrire un SMS"))
        send = Gtk.Button(label=_t("Envoyer"))
        send.get_style_context().add_class("suggested-action")
        send.set_sensitive(bool(device.get("connected")))

        def deliver(*_):
            targets = [number.get_text().strip()] if number else addresses
            if entry.get_text().strip() and targets and targets[0]:
                self.act({"cmd": "sms", "device": device["id"], "addresses": targets, "text": entry.get_text()})
        entry.connect("activate", deliver)
        send.connect("clicked", deliver)
        line.pack_start(entry, True, True, 0)
        line.pack_start(send, False, False, 0)
        right.pack_start(line, False, False, 0)
        return paned

    def open_thread(self, device, thread):
        self.thread = thread
        request({"cmd": "thread", "device": device["id"], "thread": thread})
        self.build()

    def files_page(self, device):
        Gtk = self.Gtk
        items = read_json(DATA / device["id"] / "received.json", [])
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin=16)
        line = Gtk.Box(spacing=8)
        send = Gtk.Button(label=_t("Envoyer un fichier…"))
        send.set_sensitive(bool(device.get("connected")))
        send.connect("clicked", lambda *_: self.choose_files(device["id"]))
        line.pack_start(send, False, False, 0)
        folder = Gtk.Button(label=_t("Ouvrir Téléchargements"))
        folder.connect("clicked", lambda *_: open_path(download_dir()))
        line.pack_start(folder, False, False, 0)
        box.pack_start(line, False, False, 0)
        box.pack_start(label(Gtk, _t("Fichiers reçus"), "heading"), False, False, 6)
        if not items:
            box.pack_start(label(Gtk, _t("Aucun fichier reçu pour l’instant. Sur le téléphone, choisissez Partager › KDE Connect › ce PC."), "subtitle"), False, False, 0)
        for item in reversed(items):
            row = Gtk.Box(spacing=8)
            row.get_style_context().add_class("card")
            when = time.strftime("%d/%m %H:%M", time.localtime(item.get("time", 0)))
            text = item.get("name") or item.get("text", "")
            row.pack_start(label(Gtk, f"{when} · {text}", selectable=True), True, True, 0)
            if item.get("kind") == "file" and Path(item.get("path", "")).exists():
                button = Gtk.Button(label=_t("Ouvrir le dossier"))
                button.connect("clicked", lambda *_, p=item["path"]: open_path(Path(p).parent))
                row.pack_start(button, False, False, 0)
            elif item.get("kind") == "url":
                button = Gtk.Button(label=_t("Ouvrir"))
                button.connect("clicked", lambda *_, u=item["text"]: open_path(u))
                row.pack_start(button, False, False, 0)
            box.pack_start(row, False, False, 0)
        return self.scrolled(box)

    def choose_files(self, device_id):
        Gtk = self.Gtk
        dialog = Gtk.FileChooserDialog(title=_t("Envoyer un fichier…"), parent=self.window,
                                       action=Gtk.FileChooserAction.OPEN)
        dialog.add_buttons(_t("Annuler"), Gtk.ResponseType.CANCEL, _t("Envoyer"), Gtk.ResponseType.OK)
        dialog.set_select_multiple(True)
        if dialog.run() == Gtk.ResponseType.OK:
            for path in dialog.get_filenames():
                self.act({"cmd": "send", "device": device_id, "path": path})
        dialog.destroy()


def gui(page=None, thread=None):
    Gtk, GLib = _gtk()
    PhoneApp(Gtk, GLib, page, thread)
    Gtk.main()
    return 0


# ── Ligne de commande ──────────────────────────────────────────────────────

def main(argv):
    if not argv or argv[0] in ("-h", "--help", "help"):
        print(__doc__)
        return 0
    command, args = argv[0], argv[1:]
    if command == "serve":
        return serve("--if-paired" in args)
    if command == "gui":
        return gui(*args[:2])
    if command == "pair-dialog" and args:
        return pair_dialog(args[0])
    if command == "reply" and len(args) >= 2:
        return reply_dialog(args[0], args[1], " ".join(args[2:]))
    if command == "paired":
        print(json.dumps(paired_devices(), ensure_ascii=False))
        return 0
    if command in ("enable", "disable"):
        value = settings()
        value["enabled"] = command == "enable"
        write_json(SETTINGS, value)
        if command == "disable":
            request({"cmd": "stop"})
            firewall(False)
        else:
            ensure_service()
        return 0
    if command == "status":
        answer = request({"cmd": "status"})
        if not answer.get("ok"):
            answer = {"ok": False, "running": False, "devices": [
                {"id": key, "name": value.get("name", key), "paired": True, "connected": False}
                for key, value in paired_devices().items()]}
        print(json.dumps(answer, ensure_ascii=False, indent=1))
        return 0
    if command in ("pair", "accept", "reject", "unpair", "ring", "refresh") and args:
        if command == "unpair" and not request({"cmd": "status"}, timeout=2).get("ok"):
            devices = read_json(DEVICES, {})
            devices.pop(args[0], None)
            write_json(DEVICES, devices)
            shutil.rmtree(DATA / args[0], ignore_errors=True)
            return 0
        message = {"cmd": command, "device": args[0]}
    elif command == "send" and len(args) >= 2:
        result = 0
        for path in args[1:]:
            answer = request({"cmd": "send", "device": args[0], "path": os.path.abspath(path)})
            if not answer.get("ok"):
                print(answer.get("error", ""), file=sys.stderr)
                result = 1
        return result
    elif command == "sms" and len(args) >= 3:
        message = {"cmd": "sms", "device": args[0], "addresses": [args[1]], "text": " ".join(args[2:])}
    elif command == "set" and len(args) == 3 and args[1] in PREFS and args[2] in ("on", "off"):
        message = {"cmd": "set", "device": args[0], "key": args[1], "value": args[2] == "on"}
    elif command == "dismiss" and len(args) == 2:
        message = {"cmd": "dismiss", "device": args[0], "notification": args[1]}
    elif command == "thread" and len(args) == 2:
        message = {"cmd": "thread", "device": args[0], "thread": args[1]}
    elif command == "announce":
        message = {"cmd": "announce"}
    elif command == "stop":
        message = {"cmd": "stop"}
    else:
        print(__doc__, file=sys.stderr)
        return 2
    answer = request(message)
    if not answer.get("ok"):
        print(answer.get("error", ""), file=sys.stderr)
        return 1
    extra = {key: value for key, value in answer.items() if key != "ok"}
    if extra:
        print(json.dumps(extra, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
