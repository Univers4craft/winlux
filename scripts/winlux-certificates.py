#!/usr/bin/python3
"""Certificats et heure de Winlux : ne jamais être refusé par un site, une box
ou un Wi-Fi d'entreprise pour une raison évitable.

* Magasin d'autorités du système : celui de ca-certificates (Mozilla), dans
  /etc/ssl/certs/ca-certificates.crt. Un certificat d'entreprise ou d'école
  s'y ajoute depuis Paramètres > Confidentialité et sécurité > Certificats,
  sans ligne de commande : copié dans /usr/local/share/ca-certificates/winlux,
  puis update-ca-certificates. Firefox le reçoit par sa politique
  d'entreprise (/etc/firefox/policies), Chrome, Edge, Brave et les autres
  navigateurs NSS par la base du compte (~/.pki/nssdb, certutil), à
  l'ouverture de session. Le Wi-Fi d'entreprise (« Certificats du système »)
  et curl, Python, APT le voient aussitôt.
* Heure : une date fausse fait échouer tous les certificats. Le démon
  `time-daemon` (lancé en root par les services système, avant le bureau)
  règle l'horloge par SNTP dès que le réseau répond, puis toutes les 6 h.
  Client maison, sans service tiers à installer ; serveurs pool.ntp.org,
  désactivable dans Paramètres > Heure et langue.
* Diagnostic TLS : `check HÔTE` dit en clair, dans les quatre langues,
  pourquoi une connexion sécurisée échoue : date du système fausse,
  certificat expiré, autorité inconnue, nom qui ne correspond pas, ou
  portail captif (Wi-Fi public) qui intercepte la connexion. Rien n'est
  contacté sans une action de l'utilisateur.

  winlux-certificates status                 état (JSON)
  winlux-certificates add FICHIER            (root) ajoute une autorité (.crt .pem .cer .der)
  winlux-certificates remove NOM             (root) retire une autorité ajoutée
  winlux-certificates time auto on|off       (root) heure réglée automatiquement
  winlux-certificates time sync              (root) synchroniser maintenant
  winlux-certificates time-daemon            (root) réglage de l'heure en continu
  winlux-certificates check HÔTE             diagnostic d'une connexion sécurisée
  winlux-certificates sync-user              autorités ajoutées → navigateurs NSS du compte
"""
import json
import os
from pathlib import Path
import re
import shutil
import socket
import ssl
import struct
import subprocess
import sys
import tempfile
import time

SYS_ROOT = Path(os.environ.get("WINLUX_SYS_ROOT", "/"))
LOCAL_CA = Path(os.environ.get("WINLUX_LOCAL_CA", str(SYS_ROOT / "usr/local/share/ca-certificates/winlux")))
BUNDLE = Path(os.environ.get("WINLUX_CA_BUNDLE", str(SYS_ROOT / "etc/ssl/certs/ca-certificates.crt")))
FIREFOX_POLICIES = SYS_ROOT / "etc/firefox/policies/policies.json"
TIME_CONF = SYS_ROOT / "etc/winlux/time.conf"
RUN = Path(os.environ.get("WINLUX_CERT_RUN", "/run/winlux"))
TIME_STATE = RUN / "time-sync.json"
RELAY_SOCKET = os.environ.get("WINLUX_ROOT_SOCKET", "/run/winlux-root.sock")
NTP_SERVERS = tuple(filter(None, os.environ.get("WINLUX_NTP_SERVERS", "").split(","))) or (
    "0.pool.ntp.org", "1.pool.ntp.org", "2.pool.ntp.org", "3.pool.ntp.org")
NTP_PORT = int(os.environ.get("WINLUX_NTP_PORT", "123"))
PORTAL_URL = os.environ.get("WINLUX_PORTAL_PROBE", "http://example.com/")
CERT_MAX = 1 << 20
NTP_EPOCH = 2208988800
# Plancher de la date : aucune date réelle n'est antérieure à la construction
# de ce Winlux (date du fichier installé), ni au 1er janvier 2026.
FLOOR = 1767225600


# ── Langue ──────────────────────────────────────────────────────────────────

def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="): code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}
_TR = {
    # Diagnostic TLS
    "La connexion sécurisée à {host} fonctionne : son certificat est valide.": (
        "The secure connection to {host} works: its certificate is valid.",
        "La conexión segura con {host} funciona: su certificado es válido.",
        "Die sichere Verbindung zu {host} funktioniert: Das Zertifikat ist gültig."),
    "La date du système est fausse ({date}). Les sites sécurisés refusent la connexion tant qu’elle n’est pas corrigée : activez « Régler l’heure automatiquement » dans Paramètres > Heure et langue.": (
        "The system date is wrong ({date}). Secure sites refuse the connection until it is fixed: turn on “Set the time automatically” in Settings > Time & language.",
        "La fecha del sistema es incorrecta ({date}). Los sitios seguros rechazan la conexión hasta corregirla: active «Establecer la hora automáticamente» en Configuración > Hora e idioma.",
        "Das Systemdatum ist falsch ({date}). Sichere Websites verweigern die Verbindung, bis es korrigiert ist: Aktivieren Sie „Uhrzeit automatisch festlegen“ unter Einstellungen > Zeit und Sprache."),
    "Le certificat de {host} a expiré. Le problème vient du site, pas de votre ordinateur : réessayez plus tard ou prévenez son responsable.": (
        "The certificate of {host} has expired. The problem is on the site’s side, not your computer’s: try again later or tell its owner.",
        "El certificado de {host} ha caducado. El problema es del sitio, no de su equipo: inténtelo más tarde o avise a su responsable.",
        "Das Zertifikat von {host} ist abgelaufen. Das Problem liegt bei der Website, nicht bei Ihrem Computer: Versuchen Sie es später erneut oder informieren Sie den Betreiber."),
    "Le certificat de {host} est signé par une autorité que Winlux ne connaît pas. Sur un réseau d’entreprise ou d’école, ajoutez le certificat fourni par le service informatique dans Paramètres > Confidentialité et sécurité > Certificats.": (
        "The certificate of {host} is signed by an authority Winlux does not know. On a company or school network, add the certificate provided by IT in Settings > Privacy & security > Certificates.",
        "El certificado de {host} está firmado por una autoridad que Winlux no conoce. En una red de empresa o de centro educativo, añada el certificado que le dé el servicio informático en Configuración > Privacidad y seguridad > Certificados.",
        "Das Zertifikat von {host} ist von einer Stelle signiert, die Winlux nicht kennt. In einem Firmen- oder Schulnetz fügen Sie das Zertifikat der IT-Abteilung unter Einstellungen > Datenschutz und Sicherheit > Zertifikate hinzu."),
    "Le certificat présenté n’est pas celui de {host} (le nom ne correspond pas). Ne continuez pas : la connexion est peut-être interceptée.": (
        "The certificate presented is not the one for {host} (the name does not match). Do not continue: the connection may be intercepted.",
        "El certificado presentado no es el de {host} (el nombre no coincide). No continúe: puede que la conexión esté interceptada.",
        "Das vorgelegte Zertifikat gehört nicht zu {host} (der Name stimmt nicht). Fahren Sie nicht fort: Die Verbindung wird möglicherweise abgefangen."),
    "Ce réseau demande d’abord de se connecter sur sa page (Wi-Fi public, hôtel, gare) : il intercepte les sites sécurisés en attendant. Ouvrez la page de connexion du réseau, acceptez ses conditions, puis réessayez.": (
        "This network first asks you to sign in on its page (public Wi-Fi, hotel, station) and intercepts secure sites until then. Open the network’s sign-in page, accept its terms, then try again.",
        "Esta red pide primero iniciar sesión en su página (Wi-Fi público, hotel, estación) e intercepta los sitios seguros mientras tanto. Abra la página de inicio de sesión de la red, acepte sus condiciones y vuelva a intentarlo.",
        "Dieses Netz verlangt zuerst eine Anmeldung auf seiner Seite (öffentliches WLAN, Hotel, Bahnhof) und fängt sichere Websites bis dahin ab. Öffnen Sie die Anmeldeseite des Netzes, akzeptieren Sie die Bedingungen und versuchen Sie es erneut."),
    "Le nom {host} est introuvable : vérifiez la connexion à Internet.": (
        "The name {host} cannot be found: check the Internet connection.",
        "No se encuentra el nombre {host}: compruebe la conexión a Internet.",
        "Der Name {host} wurde nicht gefunden: Prüfen Sie die Internetverbindung."),
    "{host} ne répond pas : vérifiez la connexion à Internet.": (
        "{host} does not respond: check the Internet connection.",
        "{host} no responde: compruebe la conexión a Internet.",
        "{host} antwortet nicht: Prüfen Sie die Internetverbindung."),
    "La connexion sécurisée avec {host} a échoué (le serveur a refusé le chiffrement).": (
        "The secure connection to {host} failed (the server refused the encryption).",
        "La conexión segura con {host} ha fallado (el servidor rechazó el cifrado).",
        "Die sichere Verbindung zu {host} ist fehlgeschlagen (der Server hat die Verschlüsselung abgelehnt)."),
    "Le certificat de {host} n’est pas valable (il est mal construit ou révoqué). Le problème vient du site : prévenez son responsable.": (
        "The certificate of {host} is not valid (badly built or revoked). The problem is on the site’s side: tell its owner.",
        "El certificado de {host} no es válido (está mal construido o revocado). El problema es del sitio: avise a su responsable.",
        "Das Zertifikat von {host} ist ungültig (fehlerhaft erstellt oder widerrufen). Das Problem liegt bei der Website: Informieren Sie den Betreiber."),
    # Ajout d'un certificat
    "Ce fichier n’est pas un certificat (formats acceptés : .crt, .pem, .cer, .der).": (
        "This file is not a certificate (accepted formats: .crt, .pem, .cer, .der).",
        "Este archivo no es un certificado (formatos aceptados: .crt, .pem, .cer, .der).",
        "Diese Datei ist kein Zertifikat (unterstützte Formate: .crt, .pem, .cer, .der)."),
    "Un fichier .p12 ou .pfx est un certificat personnel : choisissez-le dans la page Wi-Fi, à la connexion au réseau d’entreprise (méthode TLS).": (
        "A .p12 or .pfx file is a personal certificate: choose it on the Wi-Fi page when joining the company network (TLS method).",
        "Un archivo .p12 o .pfx es un certificado personal: elíjalo en la página Wi-Fi al conectarse a la red de empresa (método TLS).",
        "Eine .p12- oder .pfx-Datei ist ein persönliches Zertifikat: Wählen Sie es auf der WLAN-Seite beim Verbinden mit dem Firmennetz (Methode TLS)."),
    "Fichier introuvable ou illisible.": ("File not found or unreadable.", "Archivo no encontrado o ilegible.", "Datei nicht gefunden oder nicht lesbar."),
    "L’opération a échoué.": ("The operation failed.", "La operación ha fallado.", "Der Vorgang ist fehlgeschlagen."),
    "Aucun serveur de temps n’a répondu. Vérifiez la connexion à Internet.": (
        "No time server answered. Check the Internet connection.",
        "Ningún servidor de hora ha respondido. Compruebe la conexión a Internet.",
        "Kein Zeitserver hat geantwortet. Prüfen Sie die Internetverbindung."),
    # Page de Paramètres
    "Certificats": ("Certificates", "Certificados", "Zertifikate"),
    "Date et heure": ("Date & time", "Fecha y hora", "Datum und Uhrzeit"),
    "Régler l’heure automatiquement": ("Set the time automatically", "Establecer la hora automáticamente", "Uhrzeit automatisch festlegen"),
    "Interroge les serveurs de temps pool.ntp.org dès que le réseau répond, puis toutes les 6 heures. Une date fausse fait refuser tous les sites sécurisés.": (
        "Asks the pool.ntp.org time servers as soon as the network answers, then every 6 hours. A wrong date makes every secure site refuse the connection.",
        "Consulta los servidores de hora pool.ntp.org en cuanto responde la red y luego cada 6 horas. Una fecha incorrecta hace que todos los sitios seguros rechacen la conexión.",
        "Fragt die Zeitserver von pool.ntp.org ab, sobald das Netz antwortet, danach alle 6 Stunden. Ein falsches Datum lässt alle sicheren Websites die Verbindung verweigern."),
    "Synchroniser maintenant": ("Sync now", "Sincronizar ahora", "Jetzt synchronisieren"),
    "Dernière synchronisation : {when}": ("Last sync: {when}", "Última sincronización: {when}", "Letzte Synchronisierung: {when}"),
    "Pas encore synchronisée.": ("Not synced yet.", "Aún no sincronizada.", "Noch nicht synchronisiert."),
    "Heure synchronisée.": ("Time synced.", "Hora sincronizada.", "Uhrzeit synchronisiert."),
    "Date du système : {date}": ("System date: {date}", "Fecha del sistema: {date}", "Systemdatum: {date}"),
    "Autorités de certification": ("Certificate authorities", "Autoridades de certificación", "Zertifizierungsstellen"),
    "{count} autorités reconnues (liste de Mozilla, mise à jour avec le système).": (
        "{count} authorities trusted (Mozilla list, updated with the system).",
        "{count} autoridades reconocidas (lista de Mozilla, actualizada con el sistema).",
        "{count} vertrauenswürdige Stellen (Mozilla-Liste, mit dem System aktualisiert)."),
    "Ajoutées par vous": ("Added by you", "Añadidas por usted", "Von Ihnen hinzugefügt"),
    "Aucune autorité ajoutée.": ("No authority added.", "Ninguna autoridad añadida.", "Keine Stelle hinzugefügt."),
    "Ajouter un certificat d’entreprise ou d’école…": ("Add a company or school certificate…", "Añadir un certificado de empresa o de centro educativo…",
                                                       "Firmen- oder Schulzertifikat hinzufügen…"),
    "Pour un réseau, un proxy ou un intranet d’entreprise ou d’école : le service informatique fournit ce fichier. Il sert ensuite au Wi-Fi, aux navigateurs et aux mises à jour.": (
        "For a company or school network, proxy or intranet: IT provides this file. It is then used by Wi-Fi, browsers and updates.",
        "Para una red, un proxy o una intranet de empresa o de centro educativo: el servicio informático le da este archivo. Después lo usan el Wi-Fi, los navegadores y las actualizaciones.",
        "Für ein Firmen- oder Schulnetz, einen Proxy oder ein Intranet: Die IT-Abteilung stellt diese Datei bereit. Sie wird dann von WLAN, Browsern und Updates verwendet."),
    "Retirer": ("Remove", "Quitar", "Entfernen"),
    "Parcourir…": ("Browse…", "Examinar…", "Durchsuchen…"),
    "Expire le {date}": ("Expires on {date}", "Caduca el {date}", "Läuft ab am {date}"),
    "Certificat ajouté : {name}.": ("Certificate added: {name}.", "Certificado añadido: {name}.", "Zertifikat hinzugefügt: {name}."),
    "Certificat retiré.": ("Certificate removed.", "Certificado quitado.", "Zertifikat entfernt."),
    "Certificats personnels (Wi-Fi d’entreprise)": ("Personal certificates (company Wi-Fi)", "Certificados personales (Wi-Fi de empresa)",
                                                  "Persönliche Zertifikate (Firmen-WLAN)"),
    "Un fichier .p12 se choisit à la connexion au réseau, dans la page Wi-Fi (méthode TLS). Winlux en garde une copie protégée.": (
        "A .p12 file is chosen when joining the network, on the Wi-Fi page (TLS method). Winlux keeps a protected copy.",
        "Un archivo .p12 se elige al conectarse a la red, en la página Wi-Fi (método TLS). Winlux guarda una copia protegida.",
        "Eine .p12-Datei wird beim Verbinden mit dem Netz auf der WLAN-Seite gewählt (Methode TLS). Winlux bewahrt eine geschützte Kopie auf."),
    "Réseaux Wi-Fi…": ("Wi-Fi networks…", "Redes Wi-Fi…", "WLAN-Netze…"),
    "Tester un site sécurisé": ("Test a secure site", "Probar un sitio seguro", "Sichere Website testen"),
    "Si un site, une mise à jour ou la Boutique refuse de se connecter, Winlux dit pourquoi.": (
        "If a site, an update or the Store refuses to connect, Winlux tells you why.",
        "Si un sitio, una actualización o la Tienda no se conecta, Winlux le dice por qué.",
        "Wenn eine Website, ein Update oder der Store keine Verbindung herstellt, sagt Winlux, warum."),
    "Adresse (par exemple github.com)": ("Address (for example github.com)", "Dirección (por ejemplo github.com)", "Adresse (zum Beispiel github.com)"),
    "Tester": ("Test", "Probar", "Testen"),
    "Test en cours…": ("Testing…", "Probando…", "Test läuft…"),
    "Page de connexion du réseau": ("Network sign-in page", "Página de inicio de sesión de la red", "Anmeldeseite des Netzes"),
    "Certificats (.crt, .pem, .cer, .der)": ("Certificates (.crt, .pem, .cer, .der)", "Certificados (.crt, .pem, .cer, .der)",
                                            "Zertifikate (.crt, .pem, .cer, .der)"),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


def done(**values):
    return dict(ok=True, **values)


def fail(error, detail="", **values):
    return dict(ok=False, error=error, detail=detail, **values)


# ── Heure ───────────────────────────────────────────────────────────────────

def now():
    fake = os.environ.get("WINLUX_CLOCK_NOW")
    return float(fake) if fake else time.time()


def clock_floor():
    try: built = Path(__file__).stat().st_mtime - 86400
    except OSError: built = 0
    return max(FLOOR, built)


def clock_wrong(moment=None):
    """Date manifestement fausse : avant la construction de Winlux, ou plus de 20 ans après."""
    moment = now() if moment is None else moment
    floor = clock_floor()
    return moment < floor or moment > floor + 20 * 365 * 86400


def time_auto():
    try: text = TIME_CONF.read_text()
    except OSError: return True
    return not re.search(r"^AUTO=0\s*$", text, re.M)


def set_time_auto(on):
    lines = []
    try: lines = [line for line in TIME_CONF.read_text().splitlines() if not line.startswith("AUTO=")]
    except OSError: pass
    lines.append(f"AUTO={1 if on else 0}")
    TIME_CONF.parent.mkdir(parents=True, exist_ok=True)
    temporary = TIME_CONF.with_name(TIME_CONF.name + f".{os.getpid()}")
    temporary.write_text("\n".join(lines) + "\n")
    os.chmod(temporary, 0o644)
    temporary.replace(TIME_CONF)
    return done(auto=on)


def time_state():
    try: return json.loads(TIME_STATE.read_text())
    except (OSError, ValueError): return {}


def sntp_query(server, timeout=4.0):
    """(décalage en secondes, aller-retour) donnés par un serveur SNTP. Le
    champ « transmit » de la question est aléatoire et doit revenir à
    l'identique : une réponse forgée par un tiers est rejetée."""
    family, kind, proto, _, address = socket.getaddrinfo(server, NTP_PORT, type=socket.SOCK_DGRAM)[0]
    nonce = os.urandom(8)
    packet = b"\x23" + b"\0" * 39 + nonce   # LI=0, version 4, mode client
    with socket.socket(family, kind, proto) as sock:
        sock.settimeout(timeout)
        sent = time.time()
        sock.sendto(packet, address)
        data, _ = sock.recvfrom(512)
        received = time.time()
    if len(data) < 48: raise ValueError("réponse trop courte")
    leap, mode, stratum = data[0] >> 6, data[0] & 7, data[1]
    if mode != 4 or leap == 3 or not 1 <= stratum <= 15 or data[24:32] != nonce:
        raise ValueError("réponse refusée")

    def stamp(raw):
        seconds, fraction = struct.unpack("!II", raw)
        if seconds < 0x80000000: seconds += 1 << 32     # ère NTP 1 (après 2036)
        return seconds - NTP_EPOCH + fraction / 2 ** 32
    server_received, server_sent = stamp(data[32:40]), stamp(data[40:48])
    offset = ((server_received - sent) + (server_sent - received)) / 2
    return offset, (received - sent) - (server_sent - server_received)


def sync_once(apply=True):
    """Interroge les serveurs, règle l'horloge (root) si l'écart dépasse 0,5 s."""
    offsets, errors = [], []
    for server in NTP_SERVERS:
        try:
            offset, delay = sntp_query(server)
            if delay < 5: offsets.append((offset, server))
        except (OSError, ValueError) as error:
            errors.append(f"{server}: {error}")
        if len(offsets) >= 3: break
    state = time_state()
    if not offsets:
        state.update(error="no-server", detail="; ".join(errors)[-500:], tried=int(time.time()))
        write_time_state(state)
        return fail("no-server", "; ".join(errors)[-500:])
    offsets.sort()
    offset, server = offsets[len(offsets) // 2]
    stepped = False
    if apply and abs(offset) > 0.5:
        time.clock_settime(time.CLOCK_REALTIME, time.time() + offset)
        stepped = True
    state = {"last": int(time.time()), "offset": round(offset, 3), "server": server, "stepped": stepped}
    write_time_state(state)
    return done(**state)


def write_time_state(state):
    try:
        RUN.mkdir(parents=True, exist_ok=True)
        temporary = TIME_STATE.with_name(TIME_STATE.name + ".new")
        temporary.write_text(json.dumps(state))
        os.chmod(temporary, 0o644)
        temporary.replace(TIME_STATE)
    except OSError:
        pass


def time_daemon():
    """Dès que le réseau répond, puis toutes les 6 h ; nouvel essai rapide en cas d'échec."""
    pause = 15
    while True:
        if time_auto():
            answer = sync_once()
            pause = 6 * 3600 if answer.get("ok") else min(300, pause * 2)
        else:
            pause = 60
        # Réglage changé ou demande « synchroniser maintenant » : réveil anticipé.
        deadline = time.monotonic() + pause
        seen = TIME_CONF.stat().st_mtime if TIME_CONF.exists() else 0
        while time.monotonic() < deadline:
            time.sleep(5)
            current = TIME_CONF.stat().st_mtime if TIME_CONF.exists() else 0
            if current != seen: break


# ── Certificats ─────────────────────────────────────────────────────────────

PEM = re.compile(rb"-----BEGIN CERTIFICATE-----\s+[A-Za-z0-9+/=\s]+?-----END CERTIFICATE-----")


def decode(pem):
    """Champs d'un certificat PEM (sujet, expiration…) ; ValueError s'il est illisible."""
    with tempfile.NamedTemporaryFile("w", suffix=".pem", delete=False) as handle:
        handle.write(pem)
        path = handle.name
    try:
        return ssl._ssl._test_decode_cert(path)
    except (ssl.SSLError, OSError) as error:
        raise ValueError(str(error)) from None
    finally:
        os.unlink(path)


def certificates_in(data):
    """Certificats PEM contenus dans un fichier PEM ou DER."""
    blocks = [block.decode("ascii") for block in PEM.findall(data)]
    if not blocks and data[:1] == b"\x30":
        blocks = [ssl.DER_cert_to_PEM_cert(data)]
    found = []
    for pem in blocks:
        try: found.append((pem.strip() + "\n", decode(pem)))
        except ValueError: continue
    return found


def _utf8(text):
    """Le décodeur de Python rend les chaînes UTF-8 du certificat octet par octet."""
    try: return text.encode("latin-1").decode("utf-8")
    except (UnicodeEncodeError, UnicodeDecodeError): return text


def subject_name(info):
    fields = {key: _utf8(value) for part in info.get("subject", ()) for key, value in part}
    return fields.get("commonName") or fields.get("organizationName") or fields.get("organizationalUnitName") or "certificat"


def expiry_text(info):
    try: return time.strftime("%d/%m/%Y", time.localtime(ssl.cert_time_to_seconds(info["notAfter"])))
    except (KeyError, ValueError): return ""


def slug(text):
    import unicodedata
    plain = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    plain = re.sub(r"[^A-Za-z0-9._-]+", "-", plain).strip("-.")[:60]
    return plain or "certificat"


def peer_can_read(path):
    """Relayé depuis un compte, root ne prend que ce que ce compte peut lire."""
    user = os.environ.get("WINLUX_PEER_USER", "")
    if os.geteuid() != 0 or not user or user == "root":
        return os.access(path, os.R_OK)
    import pwd
    try: entry = pwd.getpwnam(user)
    except KeyError: return False
    pid = os.fork()
    if pid == 0:
        try:
            os.setgroups(os.getgrouplist(user, entry.pw_gid))
            os.setgid(entry.pw_gid)
            os.setuid(entry.pw_uid)
            with open(path, "rb") as handle: handle.read(1)
            os._exit(0)
        except BaseException:
            os._exit(1)
    _, status = os.waitpid(pid, 0)
    return os.waitstatus_to_exitcode(status) == 0


def added():
    found = []
    for path in sorted(LOCAL_CA.glob("*.crt")):
        try:
            info = decode(path.read_text())
        except (OSError, ValueError):
            info = {}
        found.append({"name": path.stem, "subject": subject_name(info) if info else path.stem,
                      "expires": expiry_text(info) if info else "", "path": str(path)})
    return found


def system_count():
    try: return BUNDLE.read_bytes().count(b"-----BEGIN CERTIFICATE-----")
    except OSError: return 0


def refresh_store():
    """Reconstruit le magasin du système avec les autorités ajoutées."""
    tool = shutil.which("update-ca-certificates")
    conf = SYS_ROOT / "etc/ca-certificates.conf"
    try: enabled = sum(1 for line in conf.read_text().splitlines() if line.startswith("mozilla/"))
    except OSError: enabled = 0
    # update-ca-certificates sans sa liste (/etc/ca-certificates.conf) viderait
    # le magasin : seulement si la liste Mozilla est bien là.
    if tool and enabled >= 100 and os.environ.get("WINLUX_CA_MANUAL") != "1":
        result = subprocess.run([tool, "--fresh"], capture_output=True, text=True, timeout=120)
        if result.returncode == 0 and system_count() >= 100:
            firefox_policy()
            return True
    # Sinon : le magasin actuel, moins les ajouts précédents, plus les ajouts actuels.
    record = LOCAL_CA / ".dans-le-magasin"
    try: previous = set(PEM.findall(record.read_bytes()))
    except OSError: previous = set()
    try: current = BUNDLE.read_bytes()
    except OSError: current = b""
    kept = [block for block in PEM.findall(current) if block not in previous]
    mine = []
    for path in sorted(LOCAL_CA.glob("*.crt")):
        mine += PEM.findall(path.read_bytes())
    BUNDLE.parent.mkdir(parents=True, exist_ok=True)
    temporary = BUNDLE.with_name(BUNDLE.name + ".new")
    temporary.write_bytes(b"\n".join(kept + mine) + b"\n")
    os.chmod(temporary, 0o644)
    temporary.replace(BUNDLE)
    if LOCAL_CA.is_dir():
        record.write_bytes(b"\n".join(mine) + b"\n")
    # Dossier /etc/ssl/certs (Wi-Fi d'entreprise, CApath) : un lien par ajout,
    # puis les liens par empreinte recalculés.
    certs_dir = BUNDLE.parent
    for link in certs_dir.glob("winlux-*.pem"):
        link.unlink()
    for path in sorted(LOCAL_CA.glob("*.crt")):
        (certs_dir / f"winlux-{path.stem}.pem").symlink_to(path)
    openssl = shutil.which("openssl")
    if openssl:
        subprocess.run([openssl, "rehash", str(certs_dir)], capture_output=True, timeout=120)
    firefox_policy()
    return True


def firefox_policy():
    """Firefox a son propre magasin : sa politique d'entreprise lui donne les ajouts."""
    try: policies = json.loads(FIREFOX_POLICIES.read_text())
    except (OSError, ValueError): policies = {}
    certificates = policies.setdefault("policies", {}).setdefault("Certificates", {})
    paths = [str(path) for path in sorted(LOCAL_CA.glob("*.crt"))]
    if paths: certificates["Install"] = paths
    else: certificates.pop("Install", None)
    if not certificates: policies["policies"].pop("Certificates")
    try:
        FIREFOX_POLICIES.parent.mkdir(parents=True, exist_ok=True)
        FIREFOX_POLICIES.write_text(json.dumps(policies, indent=2))
        os.chmod(FIREFOX_POLICIES, 0o644)
    except OSError:
        pass


def add(path):
    source = Path(path)
    if source.suffix.lower() in (".p12", ".pfx"):
        return fail("personal")
    try:
        if not source.is_absolute() or not source.is_file() or not peer_can_read(source) or source.stat().st_size > CERT_MAX:
            return fail("unreadable")
        data = source.read_bytes()
    except OSError:
        return fail("unreadable")
    found = certificates_in(data)
    if not found:
        return fail("not-certificate")
    LOCAL_CA.mkdir(parents=True, exist_ok=True)
    os.chmod(LOCAL_CA, 0o755)
    names = []
    for pem, info in found:
        name = slug(subject_name(info))
        target = LOCAL_CA / f"{name}.crt"
        index = 2
        while target.exists() and target.read_text() != pem:
            target = LOCAL_CA / f"{name}-{index}.crt"
            index += 1
        target.write_text(pem)
        os.chmod(target, 0o644)
        names.append(target.stem)
    refresh_store()
    return done(added=names, subject=subject_name(found[0][1]))


def remove(name):
    if not re.fullmatch(r"[A-Za-z0-9._-]{1,80}", name) or name.startswith("."):
        return fail("failed")
    target = LOCAL_CA / f"{name}.crt"
    if not target.is_file():
        return fail("unknown")
    target.unlink()
    refresh_store()
    return done(removed=name)


def sync_user():
    """Autorités ajoutées → base NSS du compte (Chrome, Edge, Brave, Chromium…)."""
    certutil = shutil.which("certutil")
    if certutil is None: return done(skipped="certutil")
    database = Path.home() / ".pki/nssdb"
    target = f"sql:{database}"
    if not (database / "cert9.db").exists():
        database.mkdir(parents=True, exist_ok=True)
        os.chmod(database.parent, 0o700)
        subprocess.run([certutil, "-N", "-d", target, "--empty-password"], capture_output=True, timeout=30)
    listing = subprocess.run([certutil, "-L", "-d", target], capture_output=True, text=True, timeout=30).stdout
    present = {match.strip() for match in re.findall(r"^(Winlux - .+?)\s{2,}\S*,\S*,\S*\s*$", listing, re.M)}
    wanted = {}
    for entry in added():
        wanted[f"Winlux - {entry['name']}"] = entry["path"]
    for nickname, path in wanted.items():
        if nickname not in present:
            subprocess.run([certutil, "-A", "-d", target, "-n", nickname, "-t", "C,,", "-i", path], capture_output=True, timeout=30)
    for nickname in present - set(wanted):
        subprocess.run([certutil, "-D", "-d", target, "-n", nickname], capture_output=True, timeout=30)
    return done(imported=sorted(wanted))


# ── Diagnostic d'une connexion sécurisée ────────────────────────────────────

def portal_detected(timeout=5):
    """Wi-Fi public qui intercepte : la page http neutre (IANA) revient
    redirigée ou remplacée. Appelé seulement après un échec TLS demandé par
    l'utilisateur, jamais en arrière-plan."""
    import http.client
    import urllib.parse
    url = urllib.parse.urlsplit(PORTAL_URL)
    try:
        connection = http.client.HTTPConnection(url.hostname, url.port or 80, timeout=timeout)
        connection.request("GET", url.path or "/", headers={"User-Agent": "Winlux"})
        response = connection.getresponse()
        body = response.read(65536)
        connection.close()
    except (OSError, http.client.HTTPException):
        return False
    return response.status != 200 or b"Example Domain" not in body


def classify(verify_code, clock_is_wrong, portal):
    """Code lisible d'un échec de vérification de certificat (OpenSSL X509_V_ERR_*)."""
    if clock_is_wrong or verify_code == 9:     # pas encore valide : horloge en retard
        return "clock"
    if portal and verify_code in (2, 18, 19, 20, 21, 62):
        return "portal"
    if verify_code == 10:
        return "clock" if clock_is_wrong else "expired"
    if verify_code == 62:
        return "name-mismatch"
    if verify_code in (2, 18, 19, 20, 21, 27):
        return "unknown-authority"
    return "invalid-certificate"


def check(host, port=443, timeout=8):
    host = host.strip()
    host = re.sub(r"^[a-z]+://", "", host, flags=re.I).split("/")[0]
    if ":" in host and host.count(":") == 1:
        host, port_text = host.split(":")
        port = int(port_text) if port_text.isdigit() else port
    if not re.fullmatch(r"[A-Za-z0-9.-]{1,253}", host or ""):
        return fail("bad-host")
    context = ssl.create_default_context(cafile=str(BUNDLE) if BUNDLE.exists() else None)
    try:
        with socket.create_connection((host, port), timeout=timeout) as sock:
            with context.wrap_socket(sock, server_hostname=host):
                pass
    except ssl.SSLCertVerificationError as error:
        code = classify(error.verify_code, clock_wrong(), portal_detected() if error.verify_code in (2, 18, 19, 20, 21, 62) else False)
        return fail(code, error.verify_message, host=host)
    except ssl.SSLError as error:
        code = "portal" if portal_detected() else ("clock" if clock_wrong() else "tls-failed")
        return fail(code, str(error), host=host)
    except socket.gaierror as error:
        return fail("dns", str(error), host=host)
    except OSError as error:
        return fail("unreachable", str(error), host=host)
    if clock_wrong():
        return fail("clock", "", host=host)
    return done(code="ok", host=host)


MESSAGES = {
    "ok": "La connexion sécurisée à {host} fonctionne : son certificat est valide.",
    "clock": "La date du système est fausse ({date}). Les sites sécurisés refusent la connexion tant qu’elle n’est pas corrigée : activez « Régler l’heure automatiquement » dans Paramètres > Heure et langue.",
    "expired": "Le certificat de {host} a expiré. Le problème vient du site, pas de votre ordinateur : réessayez plus tard ou prévenez son responsable.",
    "unknown-authority": "Le certificat de {host} est signé par une autorité que Winlux ne connaît pas. Sur un réseau d’entreprise ou d’école, ajoutez le certificat fourni par le service informatique dans Paramètres > Confidentialité et sécurité > Certificats.",
    "name-mismatch": "Le certificat présenté n’est pas celui de {host} (le nom ne correspond pas). Ne continuez pas : la connexion est peut-être interceptée.",
    "portal": "Ce réseau demande d’abord de se connecter sur sa page (Wi-Fi public, hôtel, gare) : il intercepte les sites sécurisés en attendant. Ouvrez la page de connexion du réseau, acceptez ses conditions, puis réessayez.",
    "dns": "Le nom {host} est introuvable : vérifiez la connexion à Internet.",
    "unreachable": "{host} ne répond pas : vérifiez la connexion à Internet.",
    "tls-failed": "La connexion sécurisée avec {host} a échoué (le serveur a refusé le chiffrement).",
    "invalid-certificate": "Le certificat de {host} n’est pas valable (il est mal construit ou révoqué). Le problème vient du site : prévenez son responsable.",
    "not-certificate": "Ce fichier n’est pas un certificat (formats acceptés : .crt, .pem, .cer, .der).",
    "personal": "Un fichier .p12 ou .pfx est un certificat personnel : choisissez-le dans la page Wi-Fi, à la connexion au réseau d’entreprise (méthode TLS).",
    "unreadable": "Fichier introuvable ou illisible.",
    "no-server": "Aucun serveur de temps n’a répondu. Vérifiez la connexion à Internet.",
}


def system_date():
    return time.strftime("%d/%m/%Y %H:%M", time.localtime(now()))


def message(answer):
    code = answer.get("code", "ok") if answer.get("ok") else answer.get("error", "failed")
    text = MESSAGES.get(code)
    if text is None: return _t("L’opération a échoué.")
    return _t(text).format(host=answer.get("host", ""), date=system_date())


def explain_curl(code, url):
    """Message clair après un échec de curl (35 : TLS, 60 : certificat refusé), None sinon."""
    if code not in (35, 51, 60): return None
    import urllib.parse
    parts = urllib.parse.urlsplit(url)
    host = parts.hostname or ""
    if clock_wrong(): return message({"ok": False, "error": "clock", "host": host})
    return message(check(f"{host}:{parts.port}" if parts.port else host)) if host else None


# ── Côté bureau : appel en root par le relais ───────────────────────────────

def call(args, timeout=180):
    here = os.path.abspath(__file__)
    if os.geteuid() == 0 or not os.path.exists(RELAY_SOCKET) or os.environ.get("WINLUX_CERT_DIRECT") == "1":
        command = [sys.executable, "-B", here, *args]
    else:
        command = [shutil.which("winlux-root") or "/usr/bin/winlux-root", "exec", "winlux-certificates", *args]
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.SubprocessError) as error:
        return fail("relay", str(error))
    for line in reversed(result.stdout.splitlines()):
        if line.startswith("{"):
            try: return json.loads(line)
            except ValueError: break
    return fail("relay", (result.stderr or result.stdout).strip()[-500:])


def status():
    state = time_state()
    return done(system=system_count(), added=added(), clock_wrong=clock_wrong(), date=system_date(),
                time_auto=time_auto(), time_last=state.get("last"), time_error=state.get("error", ""))


def available():
    return True


def page(host):
    return CertificatesPanel(host)


def time_section(host):
    """Section « Date et heure » (Paramètres > Heure et langue, et page Certificats)."""
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GLib, Gtk
    state = status()
    host.section(_t("Date et heure"))
    box = host.card()
    if state["clock_wrong"]:
        warning = host.hint(message({"ok": False, "error": "clock"}), box)
        warning.get_style_context().add_class("error")
        warning.set_name("certificates-clock-wrong")
    note = Gtk.Label(xalign=0, wrap=True)

    def describe(answer=None):
        fresh = time_state()
        if answer is not None and not answer.get("ok"):
            note.set_text(message(answer))
        elif fresh.get("last"):
            when = time.strftime("%d/%m/%Y %H:%M", time.localtime(fresh["last"]))
            note.set_text(_t("Dernière synchronisation : {when}").format(when=when))
        else:
            note.set_text(_t("Pas encore synchronisée."))

    def background(args, after):
        import threading

        def worker():
            answer = call(args)
            GLib.idle_add(lambda: after(answer) and False)
        threading.Thread(target=worker, daemon=True).start()

    def toggled(on):
        background(["time", "auto", "on" if on else "off"],
                   lambda answer: on and answer.get("ok") and background(["time", "sync"], describe))
    switch = host.switch(box, _t("Régler l’heure automatiquement"), state["time_auto"], toggled,
                         _t("Interroge les serveurs de temps pool.ntp.org dès que le réseau répond, puis toutes les 6 heures. Une date fausse fait refuser tous les sites sécurisés."))
    switch.set_name("certificates-time-auto")
    host.hint(_t("Date du système : {date}").format(date=state["date"]), box)
    box.pack_start(note, False, False, 0)
    describe()
    host.actions(box, [(_t("Synchroniser maintenant"), lambda: background(["time", "sync"], describe))])
    box.show_all()
    return switch


class CertificatesPanel:
    """Page Certificats de Paramètres : heure, autorités, test d'un site."""

    def __init__(self, host):
        import gi
        gi.require_version("Gtk", "3.0")
        from gi.repository import GLib, Gtk
        self.Gtk, self.GLib, self.host = Gtk, GLib, host
        state = status()
        if state["clock_wrong"]:
            time_section(host)
        host.section(_t("Autorités de certification"),
                     _t("{count} autorités reconnues (liste de Mozilla, mise à jour avec le système).").format(count=state["system"]))
        box = host.card()
        self.message = Gtk.Label(xalign=0, wrap=True, selectable=True)
        self.message.set_name("certificates-message")
        box.pack_start(self.message, False, False, 0)
        host.hint(_t("Ajoutées par vous"), box)
        self.list_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        box.pack_start(self.list_box, False, False, 0)
        add_button = Gtk.Button(label=_t("Ajouter un certificat d’entreprise ou d’école…"))
        add_button.set_name("certificates-add")
        add_button.connect("clicked", lambda *_: self.choose())
        host.row(box, _t("Ajouter un certificat d’entreprise ou d’école…"), add_button,
                 _t("Pour un réseau, un proxy ou un intranet d’entreprise ou d’école : le service informatique fournit ce fichier. Il sert ensuite au Wi-Fi, aux navigateurs et aux mises à jour."))
        add_button.set_label(_t("Parcourir…"))
        host.section(_t("Certificats personnels (Wi-Fi d’entreprise)"),
                     _t("Un fichier .p12 se choisit à la connexion au réseau, dans la page Wi-Fi (méthode TLS). Winlux en garde une copie protégée."))
        wifi = host.card()
        host.actions(wifi, [(_t("Réseaux Wi-Fi…"), lambda: GLib.idle_add(host.open_page, 35))])
        host.section(_t("Tester un site sécurisé"), _t("Si un site, une mise à jour ou la Boutique refuse de se connecter, Winlux dit pourquoi."))
        test = host.card()
        line = Gtk.Box(spacing=8)
        self.address = Gtk.Entry(placeholder_text=_t("Adresse (par exemple github.com)"))
        self.address.set_name("certificates-address")
        self.address.set_hexpand(True)
        self.address.connect("activate", lambda *_: self.test())
        line.pack_start(self.address, True, True, 0)
        button = Gtk.Button(label=_t("Tester"))
        button.connect("clicked", lambda *_: self.test())
        line.pack_start(button, False, False, 0)
        test.pack_start(line, False, False, 0)
        self.result = Gtk.Label(xalign=0, wrap=True, selectable=True)
        self.result.set_name("certificates-result")
        test.pack_start(self.result, False, False, 0)
        self.portal = Gtk.Button(label=_t("Page de connexion du réseau"))
        self.portal.set_name("certificates-portal")
        self.portal.set_no_show_all(True)
        self.portal.connect("clicked", lambda *_: subprocess.Popen(["xdg-open", PORTAL_URL], stdout=subprocess.DEVNULL,
                                                                    stderr=subprocess.DEVNULL, start_new_session=True))
        test.pack_start(self.portal, False, False, 0)
        self.show_added(state["added"])
        self.say("")

    def say(self, text, error=False):
        self.message.set_text(text)
        context = self.message.get_style_context()
        (context.add_class if error else context.remove_class)("error")
        self.message.set_visible(bool(text))

    def show_added(self, entries):
        Gtk = self.Gtk
        for child in self.list_box.get_children(): child.destroy()
        if not entries:
            self.host.hint(_t("Aucune autorité ajoutée."), self.list_box)
        for entry in entries:
            line = Gtk.Box(spacing=8)
            texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
            texts.pack_start(Gtk.Label(label=entry["subject"], xalign=0, wrap=True), False, False, 0)
            if entry.get("expires"):
                self.host.hint(_t("Expire le {date}").format(date=entry["expires"]), texts)
            line.pack_start(texts, True, True, 0)
            remove_button = Gtk.Button(label=_t("Retirer"))
            remove_button.connect("clicked", lambda *_, name=entry["name"]: self.remove(name))
            line.pack_start(remove_button, False, False, 0)
            self.list_box.pack_start(line, False, False, 0)
        self.list_box.show_all()

    def background(self, args, after):
        import threading

        def worker():
            answer = call(args)
            self.GLib.idle_add(lambda: after(answer) and False)
        threading.Thread(target=worker, daemon=True).start()

    def choose(self):
        Gtk = self.Gtk
        dialog = Gtk.FileChooserNative.new(_t("Ajouter un certificat d’entreprise ou d’école…"), self.host,
                                           Gtk.FileChooserAction.OPEN, None, None)
        certificates = Gtk.FileFilter()
        certificates.set_name(_t("Certificats (.crt, .pem, .cer, .der)"))
        for pattern in ("*.crt", "*.pem", "*.cer", "*.der", "*.CRT", "*.PEM", "*.CER", "*.DER"):
            certificates.add_pattern(pattern)
        dialog.add_filter(certificates)
        if dialog.run() == Gtk.ResponseType.ACCEPT and dialog.get_filename():
            self.add_file(dialog.get_filename())
        dialog.destroy()

    def add_file(self, path):
        def after(answer):
            if answer.get("ok"):
                self.say(_t("Certificat ajouté : {name}.").format(name=answer.get("subject", "")))
                subprocess.Popen([sys.executable, "-B", os.path.abspath(__file__), "sync-user"],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            else:
                self.say(message(answer), True)
            self.show_added(added())
        self.background(["add", path], after)

    def remove(self, name):
        def after(answer):
            self.say(_t("Certificat retiré.") if answer.get("ok") else message(answer), not answer.get("ok"))
            if answer.get("ok"):
                subprocess.Popen([sys.executable, "-B", os.path.abspath(__file__), "sync-user"],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            self.show_added(added())
        self.background(["remove", name], after)

    def test(self):
        import threading
        host = self.address.get_text().strip()
        if not host: return
        self.result.set_text(_t("Test en cours…"))
        self.portal.hide()

        def worker():
            answer = check(host)
            self.GLib.idle_add(lambda: self.tested(answer) and False)
        threading.Thread(target=worker, daemon=True).start()

    def tested(self, answer):
        self.result.set_text(message(answer))
        context = self.result.get_style_context()
        (context.remove_class if answer.get("ok") else context.add_class)("error")
        self.portal.set_visible(answer.get("error") == "portal")


# ── Ligne de commande ───────────────────────────────────────────────────────

def main(argv):
    args = argv[1:]
    command = args[0] if args else "status"
    if command == "status" and len(args) <= 1:
        answer = status()
    elif command == "add" and len(args) == 2:
        answer = add(args[1])
    elif command == "remove" and len(args) == 2:
        answer = remove(args[1])
    elif command == "time" and args[1:] in (["auto", "on"], ["auto", "off"]):
        answer = set_time_auto(args[2] == "on")
    elif command == "time" and args[1:] == ["sync"]:
        answer = sync_once(apply=os.geteuid() == 0 and os.environ.get("WINLUX_CLOCK_NO_SET") != "1")
    elif command == "time-daemon" and len(args) == 1:
        try: time_daemon()
        except KeyboardInterrupt: pass
        return 0
    elif command == "check" and len(args) == 2:
        answer = check(args[1])
        answer["message"] = message(answer)
    elif command == "explain-curl" and len(args) == 3 and args[1].isdigit():
        text = explain_curl(int(args[1]), args[2])
        answer = done(message=text or "")
    elif command == "sync-user" and len(args) == 1:
        answer = sync_user()
    else:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    if not answer.get("ok") and "message" not in answer:
        answer["message"] = message(answer)
    print(json.dumps(answer, ensure_ascii=False))
    return 0 if answer.get("ok") else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
