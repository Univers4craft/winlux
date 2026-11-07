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
                     [--eap peap|ttls|tls] [--phase2 mschapv2|pap|gtc]
                     [--ca system|none|FICHIER] [--domain DOMAINE]
                     [--client-cert FICHIER] [--private-key FICHIER]
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
import time
import uuid

SYS_ROOT = Path(os.environ.get("WINLUX_SYS_ROOT", "/"))
CONNECTIONS = Path(os.environ.get("WINLUX_WIFI_CONNECTIONS", "/etc/NetworkManager/system-connections"))
# Certificats choisis pour le Wi-Fi d'entreprise : copiés ici (root, 0600) pour
# que le réseau se reconnecte même si le fichier d'origine est déplacé.
KEPT_CERTS = Path(os.environ.get("WINLUX_WIFI_CERTS", "/etc/winlux/wifi-certificats"))
CERT_MAX = 1 << 20
RELAY_SOCKET = os.environ.get("WINLUX_ROOT_SOCKET", "/run/winlux-root.sock")
AIRPLANE = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "winlux/airplane-mode"
SECURITIES = ("open", "wpa", "sae", "wep", "owe", "eap")
# Entreprise (802.1X) : méthodes EAP et authentifications internes proposées.
EAP_METHODS = ("peap", "ttls", "tls")
PHASE2 = ("mschapv2", "pap", "gtc")
# Certificat d'autorité « système » : celui de ca-certificates, embarqué dans l'image.
CONNECT_WAIT = 45
WPS_WAIT = 120
# Réseau disparu de la liste pendant la saisie de la clé (balayage expiré, box
# au bord de la portée) : nouveaux balayages avant d'abandonner, dans un temps
# borné qui reste sous l'attente de la page (200 s).
FIND_SCANS = 4
RETRY_PAUSE = float(os.environ.get("WINLUX_WIFI_RETRY_PAUSE", "3"))
ACTIVATE_TRIES = 3
LOST_BUDGET = float(os.environ.get("WINLUX_WIFI_LOST_BUDGET", "100"))


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
    "Méthode": ("Method", "Método", "Methode"),
    "PEAP (le plus courant)": ("PEAP (most common)", "PEAP (el más habitual)", "PEAP (am häufigsten)"),
    "TLS (certificat)": ("TLS (certificate)", "TLS (certificado)", "TLS (Zertifikat)"),
    "Authentification interne": ("Inner authentication", "Autenticación interna", "Innere Authentifizierung"),
    "Certificat du serveur": ("Server certificate", "Certificado del servidor", "Serverzertifikat"),
    "Certificats du système": ("System certificates", "Certificados del sistema", "Systemzertifikate"),
    "Choisir un fichier…": ("Choose a file…", "Elegir un archivo…", "Datei auswählen…"),
    "Ne pas vérifier (déconseillé)": ("Do not verify (not recommended)", "No verificar (no recomendado)", "Nicht prüfen (nicht empfohlen)"),
    "Fichier du certificat": ("Certificate file", "Archivo del certificado", "Zertifikatsdatei"),
    "Domaine (facultatif)": ("Domain (optional)", "Dominio (opcional)", "Domäne (optional)"),
    "Certificat personnel": ("Personal certificate", "Certificado personal", "Persönliches Zertifikat"),
    "Clé privée (si à part)": ("Private key (if separate)", "Clave privada (si está aparte)", "Privater Schlüssel (falls separat)"),
    "Clé privée": ("Private key", "Clave privada", "Privater Schlüssel"),
    "Mot de passe de la clé privée (facultatif)": ("Private key password (optional)", "Contraseña de la clave privada (opcional)", "Kennwort des privaten Schlüssels (optional)"),
    "Entrez votre nom d’utilisateur et choisissez votre certificat.": ("Enter your user name and choose your certificate.", "Escriba su nombre de usuario y elija su certificado.", "Geben Sie Ihren Benutzernamen ein und wählen Sie Ihr Zertifikat."),
    "Un des fichiers de certificat choisis est introuvable.": ("One of the chosen certificate files cannot be found.", "No se encuentra uno de los archivos de certificado elegidos.", "Eine der gewählten Zertifikatsdateien wurde nicht gefunden."),
    "Page de connexion du réseau": ("Network sign-in page", "Página de inicio de sesión de la red", "Anmeldeseite des Netzwerks"),
    "Wi-Fi public (gare, hôtel, café) : ouvre la page où accepter les conditions ou se connecter.": ("Public Wi-Fi (station, hotel, café): opens the page to accept the terms or sign in.", "Wi-Fi público (estación, hotel, cafetería): abre la página para aceptar las condiciones o iniciar sesión.", "Öffentliches WLAN (Bahnhof, Hotel, Café): öffnet die Seite, um die Bedingungen anzunehmen oder sich anzumelden."),
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
    "Mot de passe refusé : la box n’accepte pas cette clé de sécurité. Vérifiez-la (majuscules comprises) et réessayez.": ("Password rejected: the router does not accept this security key. Check it (including capital letters) and try again.", "Contraseña rechazada: el router no acepta esta clave de seguridad. Revísela (incluidas las mayúsculas) y vuelva a intentarlo.", "Kennwort abgelehnt: Der Router akzeptiert diesen Sicherheitsschlüssel nicht. Prüfen Sie ihn (einschließlich Großbuchstaben) und versuchen Sie es erneut."),
    "Réseau « {name} » momentanément introuvable : il a disparu de la liste pendant la connexion. Votre mot de passe est gardé : rapprochez-vous de la box, puis cliquez sur « Se connecter ».": ("The network “{name}” cannot be found for the moment: it left the list during the connection. Your password is kept: move closer to the router, then click “Connect”.", "La red «{name}» no se encuentra por el momento: desapareció de la lista durante la conexión. Su contraseña se conserva: acérquese al router y haga clic en «Conectar».", "Das Netzwerk „{name}“ ist im Moment nicht auffindbar: Es ist während der Verbindung aus der Liste verschwunden. Ihr Kennwort bleibt erhalten: Gehen Sie näher an den Router und klicken Sie auf „Verbinden“."),
    "Winlux s’y connectera aussi tout seul, avec ce mot de passe, dès que le réseau réapparaîtra.": ("Winlux will also connect to it on its own, with this password, as soon as the network comes back.", "Winlux también se conectará solo, con esta contraseña, en cuanto la red vuelva a aparecer.", "Winlux verbindet sich außerdem selbst mit diesem Kennwort, sobald das Netzwerk wieder erscheint."),
    "Connecté à « {name} » avec le mot de passe que vous aviez tapé.": ("Connected to “{name}” with the password you typed.", "Conectado a «{name}» con la contraseña que escribió.", "Mit „{name}“ verbunden, mit dem eingegebenen Kennwort."),
    "La box « {name} » utilise un chiffrement que ce Wi-Fi ne prend pas en charge (GCMP-256 seul). Dans les réglages de la box, choisissez WPA2/WPA3 ou le chiffrement AES (CCMP), puis réessayez.": ("The router “{name}” uses an encryption this Wi-Fi does not support (GCMP-256 only). In the router settings, choose WPA2/WPA3 or AES (CCMP) encryption, then try again.", "El router «{name}» usa un cifrado que este Wi-Fi no admite (solo GCMP-256). En la configuración del router, elija WPA2/WPA3 o el cifrado AES (CCMP) y vuelva a intentarlo.", "Der Router „{name}“ verwendet eine Verschlüsselung, die dieses WLAN nicht unterstützt (nur GCMP-256). Wählen Sie in den Router-Einstellungen WPA2/WPA3 oder AES-Verschlüsselung (CCMP) und versuchen Sie es erneut."),
    "Recherche de « {name} »…": ("Looking for “{name}”…", "Buscando «{name}»…", "„{name}“ wird gesucht…"),
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
        (("no network with ssid", "could not be found", "not found", "ssid-not-found", "no network",
          "access point"), "out-of-range"),
        (("ip configuration", "ip-config", "dhcp"), "no-ip"),
        (("radio", "wireless is disabled", "wi-fi is disabled", "rfkill"), "wifi-off"),
        (("no wi-fi device", "no suitable device", "no wifi device", "device not found"), "no-device"),
        (("timeout", "timed out"), "timeout"),
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


AIRPLANE_MARK = Path(os.environ.get("WINLUX_AIRPLANE_MARK", "/run/winlux/airplane"))


def airplane_state():
    """Mode avion : demandé par « airplane on » (marque du service) et Wi-Fi
    toujours coupé. Couper le seul Wi-Fi n'est pas le mode avion : sans
    Bluetooth, l'ancien calcul (Wi-Fi et tout Bluetooth coupés) le disait
    actif et l'interrupteur « Mode avion » s'allumait avec le Wi-Fi."""
    return AIRPLANE_MARK.exists() and not radio_on()


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


def visible(ssid, rescan=False):
    return any(network["ssid"] == ssid for network in scan(rescan))


def find_again(ssid, deadline):
    """Le réseau n'est plus dans la liste : on rebalaye quelques fois avant de
    dire qu'il est introuvable. Vrai dès qu'il réapparaît."""
    if visible(ssid):
        return True
    for attempt in range(FIND_SCANS):
        if attempt:
            if time.monotonic() + RETRY_PAUSE > deadline:
                break
            time.sleep(RETRY_PAUSE)
        if visible(ssid, True):
            return True
    return False


def ever_connected(ident):
    """NetworkManager date chaque connexion réussie d'un profil (0 : jamais)."""
    result = nm(["-g", "connection.timestamp", "connection", "show", "uuid", ident], timeout=5)
    try:
        return int(result.stdout.strip() or 0) > 0
    except ValueError:
        return False


def profiles_for(ssid):
    return [p for p in profiles() if p["ssid"] == ssid]


def wps_configured(ies):
    """Éléments 802.11 bruts -> vrai si la box annonce le WPS à l'état
    « Configured » (élément fournisseur 00:50:F2 type 4, attribut 0x1044 = 2).
    L'élément WPS peut être découpé en plusieurs morceaux : ils sont recollés."""
    wps, index = b"", 0
    while index + 2 <= len(ies):
        kind, size = ies[index], ies[index + 1]
        body = ies[index + 2:index + 2 + size]
        index += 2 + size
        if kind == 221 and body[:4] == b"\x00\x50\xf2\x04":
            wps += body[4:]
    index = 0
    while index + 4 <= len(wps):
        kind = int.from_bytes(wps[index:index + 2], "big")
        size = int.from_bytes(wps[index + 2:index + 4], "big")
        value = wps[index + 4:index + 4 + size]
        index += 4 + size
        if kind == 0x1044:
            return value[:1] == b"\x02"
    return False


def cipher_unsupported(ssid):
    """Box WPA2/WPA3 sans CCMP ni TKIP (GCMP-256 seul, Wi-Fi 7 réglé en WPA3
    strict) : NetworkManager n'y propose que CCMP et TKIP, il ne la trouve donc
    jamais. Vrai seulement si tous ses points d'accès sont dans ce cas."""
    result = nm(["-t", "-f", "SSID,WPA-FLAGS,RSN-FLAGS", "device", "wifi", "list"], timeout=10)
    seen = False
    for line in result.stdout.splitlines():
        fields = split_terse(line)
        if len(fields) < 3 or fields[0] != ssid:
            continue
        flags = (fields[1] + " " + fields[2]).replace("(none)", "").split()
        if not flags or any(flag in flags for flag in ("pair_ccmp", "pair_tkip", "pair_wep40", "pair_wep104")):
            return False
        seen = True
    return seen


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
            # « pbc » n'apparaît que pendant l'appui sur le bouton de la box :
            # une box qui annonce le WPS « configuré » le propose dès l'ouverture.
            if (values.get("WPS") or {}).get("Type") in ("pbc", "pin") or \
                    wps_configured(bytes(values.get("IEs") or b"") + bytes(values.get("BeaconIEs") or b"")):
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


def key_problem(security, key, identity="", eap="peap", client_cert=""):
    """None si la clé convient au type de sécurité, sinon le code d'erreur.
    Entreprise TLS : la « clé » est le mot de passe de la clé privée
    (facultatif) ; il faut l'identité et le certificat client."""
    if security in ("open", "owe"): return None
    if security == "eap":
        if eap == "tls": return None if identity and client_cert else "eap-certificate"
        return None if identity and key else "eap-identity"
    if security == "wep":
        if len(key) in (5, 13) and key.isascii() and key.isprintable(): return None
        if len(key) in (10, 26) and re.fullmatch(r"[0-9A-Fa-f]+", key): return None
        return "wep-format"
    if len(key) == 64 and re.fullmatch(r"[0-9A-Fa-f]+", key): return None
    if 8 <= len(key) <= 63 and key.isascii() and all(32 <= ord(c) < 127 for c in key): return None
    return "key-format"


def eap_lines(identity, key, eap="peap", phase2="mschapv2", ca="system", domain="",
              client_cert="", private_key=""):
    """Section [802-1x] : PEAP ou TTLS (identifiant et mot de passe) ou TLS
    (certificat client). Le serveur est vérifié par les certificats du système
    (ca-certificates), par un fichier choisi, ou pas du tout si demandé."""
    lines = ["[802-1x]", f"eap={eap};", f"identity={keyfile_escape(identity)}"]
    if eap in ("peap", "ttls"):
        lines += [f"password={keyfile_escape(key)}", f"phase2-auth={phase2}"]
    else:
        lines.append(f"client-cert={keyfile_escape(client_cert)}")
        lines.append(f"private-key={keyfile_escape(private_key or client_cert)}")
        if key: lines.append(f"private-key-password={keyfile_escape(key)}")
    if ca == "system":
        lines.append("system-ca-certs=true")
    elif ca and ca != "none":
        lines.append(f"ca-cert={keyfile_escape(ca)}")
    if domain: lines.append(f"domain-suffix-match={keyfile_escape(domain)}")
    return lines + [""]


def profile_text(ssid, security, key="", identity="", hidden=False, autoconnect=True, ident=None, **eap):
    ident = ident or str(uuid.uuid4())
    lines = ["[connection]", f"id={keyfile_escape(ssid)}", f"uuid={ident}", "type=wifi",
             f"autoconnect={'true' if autoconnect else 'false'}", "",
             "[wifi]", "mode=infrastructure", f"ssid={keyfile_ssid(ssid)}"]
    if hidden: lines.append("hidden=true")
    lines.append("")
    if security == "wpa":
        # WPA2 et le mode de transition WPA2/WPA3 (WPA personnel TKIP compris) :
        # trames de gestion protégées quand la box les propose.
        lines += ["[wifi-security]", "key-mgmt=wpa-psk", "pmf=2", f"psk={keyfile_escape(key)}", ""]
    elif security == "sae":
        # WPA3 personnel (et 6 GHz, qui l'impose) : PMF obligatoire.
        lines += ["[wifi-security]", "key-mgmt=sae", "pmf=3", f"psk={keyfile_escape(key)}", ""]
    elif security == "owe":
        lines += ["[wifi-security]", "key-mgmt=owe", ""]
    elif security == "wep":
        lines += ["[wifi-security]", "key-mgmt=none", "auth-alg=open", "wep-key-type=1",
                  "wep-tx-keyidx=0", f"wep-key0={keyfile_escape(key)}", ""]
    elif security == "eap":
        lines += ["[wifi-security]", "key-mgmt=wpa-eap", "pmf=2", ""] + eap_lines(identity, key, **eap)
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


def activate_patiently(ident, ssid, hidden=False):
    """Active le profil sans jamais conclure trop vite : un réseau absent de la
    liste de NetworkManager échoue tout de suite (« no compatible access
    point »), il est alors rebalayé puis réessayé ; une clé « refusée » alors
    que la box n'est plus visible compte comme une box perdue, pas comme un
    mauvais mot de passe (poignée de main coupée)."""
    deadline = time.monotonic() + LOST_BUDGET
    error, detail = None, ""
    for attempt in range(ACTIVATE_TRIES):
        error, detail = activate(ident, ssid)
        if error == "out-of-range" and not hidden and cipher_unsupported(ssid):
            return "cipher", detail
        if error == "bad-key" and not hidden and not visible(ssid, True):
            error = "out-of-range"
        if error != "out-of-range" or hidden or time.monotonic() > deadline:
            break
        if attempt + 1 < ACTIVATE_TRIES:
            find_again(ssid, deadline)
    return error, detail


def remove_profile(ident, path=None):
    nm(["connection", "delete", "uuid", ident], timeout=10)
    if path is not None:
        try: path.unlink()
        except OSError: pass
    shutil.rmtree(KEPT_CERTS / ident, ignore_errors=True)


def peer_can_read(path):
    """Relayé depuis un compte, le service tourne en root : il ne prend un
    fichier que si ce compte peut le lire lui-même (pas /etc/shadow)."""
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


def keep_certificates(ident, eap):
    """Copie les certificats choisis dans KEPT_CERTS/<profil>/ ; renvoie les options
    qui pointent sur les copies. FileNotFoundError si un fichier manque ou n'est
    pas lisible par le compte qui le demande."""
    kept = dict(eap)
    for name in ("ca", "client_cert", "private_key"):
        value = kept.get(name, "")
        if not value or (name == "ca" and value in ("system", "none")):
            continue
        source = Path(value)
        if not source.is_absolute() or not source.is_file() or not peer_can_read(source) \
                or source.stat().st_size > CERT_MAX:
            raise FileNotFoundError(value)
        folder = KEPT_CERTS / ident
        folder.mkdir(parents=True, exist_ok=True)
        os.chmod(folder, 0o700)
        suffix = source.suffix.lower() if re.fullmatch(r"\.[a-z0-9]{1,5}", source.suffix.lower()) else ".pem"
        target = folder / (name.replace("_", "-") + suffix)
        descriptor = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(descriptor, "wb") as out:
            out.write(source.read_bytes())
        kept[name] = str(target)
    return kept


def connect(ssid, security, key="", identity="", hidden=False, autoconnect=True, **eap):
    if not ssid or len(ssid.encode("utf-8")) > 32:
        return fail("bad-ssid")
    if security not in SECURITIES:
        return fail("failed", "security")
    if security == "eap":
        if eap.get("eap", "peap") not in EAP_METHODS or eap.get("phase2", "mschapv2") not in PHASE2:
            return fail("failed", "eap")
        for name in ("client_cert", "private_key", "ca"):
            value = eap.get(name, "")
            if value and value not in ("system", "none") and not Path(value).is_file():
                return fail("eap-file", value)
    problem = key_problem(security, key, identity, eap.get("eap", "peap"), eap.get("client_cert", ""))
    if problem:
        return fail(problem)
    previous = profiles_for(ssid)
    ident = str(uuid.uuid4())
    if security == "eap":
        try:
            eap = keep_certificates(ident, eap)
        except OSError as error:
            shutil.rmtree(KEPT_CERTS / ident, ignore_errors=True)
            return fail("eap-file", str(error))
    text, ident = profile_text(ssid, security, key, identity, hidden, autoconnect, ident=ident,
                               **(eap if security == "eap" else {}))
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
    error, detail = activate_patiently(ident, ssid, hidden)
    if error == "out-of-range" and not hidden:
        # Le réseau a disparu pendant la saisie ou la connexion : ce n'est pas
        # un échec de la clé. La page garde ce qui a été tapé ; le profil reste
        # (sauf si un profil qui a déjà marché existe) pour que NetworkManager
        # s'y connecte seul quand le réseau revient.
        working = [old for old in previous if ever_connected(old["uuid"])]
        if autoconnect and not working:
            for old in previous:
                remove_profile(old["uuid"])
            return fail("lost", detail, waiting=True)
        remove_profile(ident, path)
        return fail("lost", detail, waiting=False)
    if error:
        # Une clé refusée n'est pas gardée ; l'ancien profil (s'il existait) reste.
        remove_profile(ident, path)
        return fail(error, detail)
    for old in previous:
        if old["uuid"] != ident:
            nm(["connection", "delete", "uuid", old["uuid"]], timeout=10)
            shutil.rmtree(KEPT_CERTS / old["uuid"], ignore_errors=True)
    return done(ssid=ssid)


def up(ssid):
    known = profiles_for(ssid)
    if not known:
        return fail("unknown")
    error, detail = activate_patiently(known[0]["uuid"], ssid)
    return fail(error, detail) if error else done(ssid=ssid)


def forget(ssid):
    known = profiles_for(ssid)
    for profile in known:
        nm(["connection", "delete", "uuid", profile["uuid"]], timeout=10)
        shutil.rmtree(KEPT_CERTS / profile["uuid"], ignore_errors=True)
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
        try: AIRPLANE_MARK.unlink()
        except OSError: pass
    result = nm(["radio", "wifi", "on" if on else "off"], timeout=10)
    return fail(classify(result.stderr), result.stderr) if result.returncode else done()


def airplane(on):
    try:
        if on:
            AIRPLANE_MARK.parent.mkdir(parents=True, exist_ok=True)
            AIRPLANE_MARK.touch()
        else:
            AIRPLANE_MARK.unlink()
    except OSError:
        pass
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
        # Le profil actif d'abord : la liste des réseaux (IN-USE) n'est mise à
        # jour qu'au balayage suivant, et l'en-tête restait « Non connecté »
        # pendant et après une connexion ou une reconnexion.
        current = next((p["ssid"] for p in profiles() if p["active"]), "")
        if not current:
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
        eap = {}
        names = {"--eap": "eap", "--phase2": "phase2", "--ca": "ca", "--domain": "domain",
                 "--client-cert": "client_cert", "--private-key": "private_key"}
        it = iter(options)
        for option in it:
            if option == "--security": security = next(it, "")
            elif option in names: eap[names[option]] = next(it, "")
            elif option == "--identity": identity = next(it, "")
            elif option == "--hidden": hidden = True
            elif option == "--no-auto": auto = False
            elif option == "--key-stdin": key = sys.stdin.readline().rstrip("\n")
            else: return fail("failed", "option " + option)
        return connect(ssid, security, key, identity, hidden, auto, **eap)
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
    "bad-key": "Mot de passe refusé : la box n’accepte pas cette clé de sécurité. Vérifiez-la (majuscules comprises) et réessayez.",
    "cipher": "La box « {name} » utilise un chiffrement que ce Wi-Fi ne prend pas en charge (GCMP-256 seul). Dans les réglages de la box, choisissez WPA2/WPA3 ou le chiffrement AES (CCMP), puis réessayez.",
    "lost": "Réseau « {name} » momentanément introuvable : il a disparu de la liste pendant la connexion. Votre mot de passe est gardé : rapprochez-vous de la box, puis cliquez sur « Se connecter ».",
    "key-format": "Une clé WPA compte de 8 à 63 caractères (ou 64 chiffres hexadécimaux).",
    "wep-format": "Une clé WEP compte 5 ou 13 caractères (ou 10 ou 26 chiffres hexadécimaux).",
    "eap-identity": "Entrez votre nom d’utilisateur et votre mot de passe.",
    "eap-certificate": "Entrez votre nom d’utilisateur et choisissez votre certificat.",
    "eap-file": "Un des fichiers de certificat choisis est introuvable.",
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
# « lost » n'en fait pas partie : la boîte se rouvre aussi, mais sans message
# d'échec, avec la clé déjà tapée (voir WifiPanel.connected).
KEY_ERRORS = ("bad-key", "key-format", "wep-format", "eap-identity", "eap-certificate", "eap-file")


PORTAL_URL = "http://example.com/"
# Réponse de la boîte de la clé quand le réseau attendu s'est connecté seul.
CONNECTED_MEANWHILE = 2


def open_portal():
    """Page de connexion d'un Wi-Fi public : une adresse http neutre (IANA)
    que le portail du réseau redirige vers sa page."""
    try:
        subprocess.Popen(["xdg-open", PORTAL_URL], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         start_new_session=True)
        return True
    except OSError:
        return False


def error_text(answer, ssid=""):
    code = answer.get("error", "failed")
    text = _t(ERRORS.get(code, "La connexion a échoué.")).format(name=ssid)
    if code == "lost" and answer.get("waiting"):
        text += " " + _t("Winlux s’y connectera aussi tout seul, avec ce mot de passe, dès que le réseau réapparaîtra.")
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
        # Boîte de la clé rouverte sur « réseau momentanément introuvable » :
        # (boîte, nom du réseau). Si NetworkManager s'y connecte seul entre-temps,
        # la boîte se ferme sur un succès.
        self.pending = None
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
        # Reconnexion automatique, coupure de la box : l'état suit sans clic.
        GLib.timeout_add_seconds(8, self.poll)

    def poll(self):
        if self.radio_switch.get_toplevel() is None or not self.radio_switch.get_realized():
            return self.radio_switch.get_parent() is not None
        if not self.busy: self.refresh(False)
        return True

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
        if self.pending is not None and connected and connected == self.pending[1] and on:
            self.pending[0].response(CONNECTED_MEANWHILE)
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
                # Wi-Fi public (gare, hôtel) : sa page de connexion s'ouvre
                # quand on demande une page web ordinaire (http, sans
                # chiffrement, que le portail peut intercepter). Aucun test de
                # connectivité en arrière-plan : rien n'est contacté sans le
                # demander.
                if network.get("security") in ("open", "owe"):
                    portal = Gtk.Button(label=_t("Page de connexion du réseau"))
                    portal.set_name("wifi-portal")
                    portal.set_tooltip_text(_t("Wi-Fi public (gare, hôtel, café) : ouvre la page où accepter les conditions ou se connecter."))
                    portal.set_valign(Gtk.Align.CENTER)
                    portal.connect("clicked", lambda *_: open_portal())
                    line.pack_start(portal, False, False, 0)
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

    def connecting(self, ssid, searching=False):
        self.say(_t("Recherche de « {name} »…" if searching else "Connexion à « {name} »…").format(name=ssid))

    def connected(self, answer, network, error=None, values=None):
        ssid = network["ssid"]
        if answer.get("ok"):
            self.say(_t("Connecté à « {name} ».").format(name=ssid))
            self.busy = False
            self.refresh(False)
            return
        if answer.get("error") == "lost" and values is not None:
            # Réseau perdu pendant la saisie ou la connexion : pas un échec. La
            # boîte revient avec tout ce qui a été tapé ; « Se connecter »
            # relance la recherche et la connexion avec la même clé.
            self.say("")
            self.busy = False
            self.refresh(False)
            self.key_dialog(dict(network, known=False), notice=error_text(answer, ssid), values=values)
            return
        if answer.get("error") in KEY_ERRORS or (answer.get("error") == "unknown" and network.get("security") not in ("open", "owe")):
            # Profil enregistré dont la clé ne marche plus, ou clé refusée : on la
            # redemande, avec ce qui avait été tapé pour corriger une faute.
            self.say("")
            self.key_dialog(dict(network, known=False), error_text(answer, ssid), values=values)
            return
        self.say(error_text(answer, ssid), True)
        self.refresh(False)

    def key_dialog(self, network, error=None, hidden=False, values=None, notice=None):
        """Boîte de la clé, façon Windows 11 ; clavier tactile intégré sous le champ.
        `values` : saisie précédente remise dans les champs ; `notice` : message
        sans alarme (réseau momentanément introuvable)."""
        Gtk = self.Gtk
        ssid = network.get("ssid", "")
        dialog = Gtk.Dialog(title=_t("Réseau caché") if hidden else _t("Connexion à « {name} »").format(name=ssid),
                            transient_for=self.host, modal=True)
        dialog.set_role("winlux-wifi-key")
        dialog.add_button(_t("Annuler"), Gtk.ResponseType.CANCEL)
        next_button = dialog.add_button(_t("Se connecter"), Gtk.ResponseType.OK)
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
        # Entreprise (802.1X) : méthode, authentification interne, certificat du
        # serveur, domaine, et certificat client pour TLS.
        enterprise = Gtk.Grid(column_spacing=8, row_spacing=6)
        identity_box.pack_start(enterprise, False, False, 0)

        def labelled(row, text, widget):
            enterprise.attach(Gtk.Label(label=_t(text), xalign=0), 0, row, 1, 1)
            widget.set_hexpand(True)
            enterprise.attach(widget, 1, row, 1, 1)
            return widget
        method = labelled(0, "Méthode", Gtk.ComboBoxText())
        for code, label in (("peap", "PEAP (le plus courant)"), ("ttls", "TTLS"), ("tls", "TLS (certificat)")):
            method.append(code, _t(label))
        method.set_active_id("peap")
        inner = labelled(1, "Authentification interne", Gtk.ComboBoxText())
        for code, label in (("mschapv2", "MSCHAPv2"), ("pap", "PAP"), ("gtc", "GTC")):
            inner.append(code, label)
        inner.set_active_id("mschapv2")
        ca_choice = labelled(2, "Certificat du serveur", Gtk.ComboBoxText())
        for code, label in (("system", "Certificats du système"), ("file", "Choisir un fichier…"),
                            ("none", "Ne pas vérifier (déconseillé)")):
            ca_choice.append(code, _t(label))
        ca_choice.set_active_id("system")
        ca_file = labelled(3, "Fichier du certificat", Gtk.FileChooserButton(title=_t("Certificat du serveur")))
        domain = labelled(4, "Domaine (facultatif)", Gtk.Entry())
        client_cert = labelled(5, "Certificat personnel", Gtk.FileChooserButton(title=_t("Certificat personnel")))
        private_key = labelled(6, "Clé privée (si à part)", Gtk.FileChooserButton(title=_t("Clé privée")))
        fields.update(method=method, inner=inner, ca=ca_choice, ca_file=ca_file, domain=domain,
                      client_cert=client_cert, private_key=private_key)
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

        def show_problem(text, alarm=True):
            problem.set_text(text or "")
            (problem.get_style_context().add_class if alarm else problem.get_style_context().remove_class)("error")
            problem.set_visible(bool(text))
        if notice: show_problem(notice, alarm=False)
        else: show_problem(error)
        if values:
            key.set_text(values.get("key", ""))
            fields["identity"].set_text(values.get("identity", ""))
            auto.set_active(values.get("auto", True))
            method.set_active_id(values.get("eap") or "peap")
            inner.set_active_id(values.get("phase2") or "mschapv2")
            domain.set_text(values.get("domain", ""))
            ca = values.get("ca") or "system"
            if ca in ("system", "none"):
                ca_choice.set_active_id(ca)
            else:
                ca_choice.set_active_id("file")
                ca_file.set_filename(ca)
            for chooser, name in ((client_cert, "client_cert"), (private_key, "private_key")):
                if values.get(name): chooser.set_filename(values[name])
        wps_button = None
        if network.get("wps") and not hidden:
            area.pack_start(Gtk.Label(label=_t("Vous pouvez aussi vous connecter en appuyant sur le bouton WPS de votre box."),
                                      xalign=0, wrap=True), False, False, 0)
            wps_button = Gtk.Button(label=_t("Utiliser le bouton WPS"), halign=Gtk.Align.START)
            area.pack_start(wps_button, False, False, 0)

        def current_security():
            return fields["security"].get_active_id() if hidden else security

        def eap_method():
            return method.get_active_id() or "peap"

        def update(*_):
            kind = current_security()
            identity_box.set_visible(kind == "eap")
            tls = kind == "eap" and eap_method() == "tls"
            for row, visible in ((1, not tls), (3, ca_choice.get_active_id() == "file"), (5, tls), (6, tls)):
                for column in (0, 1):
                    child = enterprise.get_child_at(column, row)
                    if child is not None: child.set_visible(visible)
            key_title.set_text(_t("Mot de passe de la clé privée (facultatif)") if tls else
                               _t("Mot de passe") if kind == "eap" else _t("Clé de sécurité"))
            key.set_sensitive(kind not in ("open", "owe"))
            ssid_ok = (not hidden) or bool(fields["ssid"].get_text())
            text = key.get_text()
            next_button.set_sensitive(ssid_ok and (kind in ("open", "owe") or
                                                   key_problem(kind, text, fields["identity"].get_text() or "x",
                                                               eap_method(), client_cert.get_filename() or "") is None))
        for widget in (key, fields["identity"], *( [fields["ssid"]] if hidden else [])):
            widget.connect("changed", update)
        if hidden: fields["security"].connect("changed", update)
        for widget in (method, ca_choice): widget.connect("changed", update)
        client_cert.connect("file-set", update)
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
        (fields["ssid"] if hidden else fields["identity"] if security == "eap" else key).grab_focus()
        if wps_button is not None:
            wps_button.connect("clicked", lambda *_: dialog.response(1))
        if notice:
            self.pending = (dialog, ssid)
        while True:
            response = dialog.run()
            if response == Gtk.ResponseType.OK:
                kind = current_security()
                problem_code = key_problem(kind, key.get_text(), fields["identity"].get_text(),
                                           eap_method(), client_cert.get_filename() or "")
                if problem_code:
                    show_problem(error_text({"error": problem_code}))
                    continue
            break
        ca_value = ca_choice.get_active_id() or "system"
        if ca_value == "file": ca_value = ca_file.get_filename() or "system"
        values = {"ssid": fields["ssid"].get_text() if hidden else ssid, "security": current_security(),
                  "key": key.get_text(), "identity": fields["identity"].get_text(), "auto": auto.get_active(),
                  "eap": eap_method(), "phase2": inner.get_active_id() or "mschapv2", "ca": ca_value,
                  "domain": domain.get_text().strip(), "client_cert": client_cert.get_filename() or "",
                  "private_key": private_key.get_filename() or ""}
        dialog.destroy()
        self.pending = None
        if response == CONNECTED_MEANWHILE:
            self.say(_t("Connecté à « {name} » avec le mot de passe que vous aviez tapé.").format(name=ssid))
            return
        if response == 1:
            self.say(_t("Appuyez maintenant sur le bouton WPS de votre box (vous avez 2 minutes)…"))
            self.run(["wps", ssid], lambda answer: self.connected(answer, network))
            return
        if response != Gtk.ResponseType.OK:
            return
        args = ["connect", values["ssid"], "--security", values["security"]]
        if hidden: args.append("--hidden")
        if not values["auto"]: args.append("--no-auto")
        if values["security"] == "eap":
            args += ["--identity", values["identity"], "--eap", values["eap"], "--ca", values["ca"]]
            if values["eap"] == "tls":
                args += ["--client-cert", values["client_cert"]]
                if values["private_key"]: args += ["--private-key", values["private_key"]]
            else:
                args += ["--phase2", values["phase2"]]
            if values["domain"]: args += ["--domain", values["domain"]]
        secret = None
        if values["security"] not in ("open", "owe") and values["key"] or values["security"] in ("wpa", "sae", "wep"):
            args.append("--key-stdin")
            secret = values["key"]
        target_network = dict(network, ssid=values["ssid"], security=values["security"])
        self.connecting(values["ssid"], searching=bool(notice))
        self.run(args, lambda answer: self.connected_hidden(answer, target_network) if hidden
                 else self.connected(answer, target_network, values=values), key=secret)

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
#winlux-osk button.osk-active, #winlux-osk button.osk-active label { background: #6a3df0; color: #ffffff; }
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
