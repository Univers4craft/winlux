#!/usr/bin/python3
"""VPN de Winlux : connexions WireGuard et OpenVPN (Paramètres > Réseau et Internet > VPN).

Deux moitiés dans un seul fichier, installé en /usr/bin/winlux-vpn :

* le service (sous-commandes ci-dessous), qui parle à NetworkManager. Il
  tourne en root : depuis le bureau d'un compte, Paramètres et les Réglages
  rapides passent par le relais `winlux-root exec winlux-vpn …`. Sans systemd
  ni logind, polkit ne voit jamais de session « active » (#377) : nmcli lancé
  par le compte serait refusé. Lancé par un compte, ce programme se relaie
  lui-même.
* la page VPN de Paramètres (classe VpnPanel), chargée par winlux-settings-pages.

  winlux-vpn status                     état et connexions (JSON)
  winlux-vpn import --stdin             ajoute une connexion ; JSON sur l'entrée
                                        standard : name, kind (wireguard|openvpn),
                                        config (texte du .conf ou du .ovpn, fichiers
                                        référencés déjà inclus), username, password,
                                        key_password
  winlux-vpn connect UUID               se connecter
  winlux-vpn disconnect UUID            se déconnecter
  winlux-vpn credentials UUID --stdin   nom d'utilisateur et mots de passe (JSON)
  winlux-vpn remove UUID                supprime la connexion et ses certificats

Logiciel libre uniquement (WireGuard intégré à NetworkManager, OpenVPN par son
greffon), aucun fournisseur proposé ni favorisé, aucun compte en ligne : on
ajoute le fichier de configuration que donne son fournisseur ou son entreprise.

Sécurité : le service ne lit jamais un fichier du compte. La page lit le .conf
ou le .ovpn (et les certificats qu'il cite) avec les droits du compte, puis les
envoie sur l'entrée standard. Les scripts d'un fichier (PostUp de WireGuard,
up/down/plugin d'OpenVPN) ne sont jamais repris : NetworkManager construit sa
propre ligne de commande. Les profils sont des fichiers système 0600
(/etc/NetworkManager/system-connections), jamais connectés automatiquement
au démarrage, comme sous Windows.

DNS : un VPN « tout le trafic » rend ses serveurs DNS exclusifs (dns-priority
négative), sans fuite vers ceux de la box. NetworkManager tourne sous umask 077
et réécrit /etc/resolv.conf en 0600 à chaque connexion (#380) : le service le
rend lisible aussitôt (winlux-network-perms), sans attendre la ronde de
winlux-network-watch.
"""
import base64
import ipaddress
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import uuid

SYS_ROOT = Path(os.environ.get("WINLUX_SYS_ROOT", "/"))
CONNECTIONS = Path(os.environ.get("WINLUX_VPN_CONNECTIONS", "/etc/NetworkManager/system-connections"))
# Certificats inclus dans un .ovpn : le greffon OpenVPN les écrit sous
# XDG_DATA_HOME/networkmanagement/certificates ; un dossier par connexion ici.
VPN_DATA = Path(os.environ.get("WINLUX_VPN_DATA", "/etc/winlux/vpn"))
RELAY_SOCKET = os.environ.get("WINLUX_ROOT_SOCKET", "/run/winlux-root.sock")
PERMS = os.environ.get("WINLUX_VPN_PERMS", "/usr/bin/winlux-network-perms")
MAX_CONFIG = 1 << 20
CONNECT_WAIT = 60
KINDS = ("wireguard", "openvpn")
UUID = re.compile(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
WG_KEY = re.compile(r"^[A-Za-z0-9+/]{42}[AEIMQUYcgkosw048]=$")


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
    "VPN": ("VPN", "VPN", "VPN"),
    "Connexions VPN": ("VPN connections", "Conexiones VPN", "VPN-Verbindungen"),
    "Un VPN fait passer le trafic de cet ordinateur par un serveur choisi : celui de votre entreprise, de votre école ou d’un fournisseur. Ajoutez le fichier de configuration qu’il vous a donné.":
        ("A VPN routes this computer's traffic through a server you choose: your company's, your school's or a provider's. Add the configuration file it gave you.",
         "Una VPN hace pasar el tráfico de este equipo por un servidor elegido: el de su empresa, su centro de estudios o un proveedor. Añada el archivo de configuración que le han dado.",
         "Ein VPN leitet den Datenverkehr dieses Computers über einen gewählten Server: den Ihres Unternehmens, Ihrer Schule oder eines Anbieters. Fügen Sie die Konfigurationsdatei hinzu, die Sie erhalten haben."),
    "Ajouter un VPN": ("Add VPN", "Agregar una VPN", "VPN hinzufügen"),
    "Aucune connexion VPN.": ("No VPN connections.", "No hay conexiones VPN.", "Keine VPN-Verbindungen."),
    "Connecté": ("Connected", "Conectado", "Verbunden"),
    "Connexion…": ("Connecting…", "Conectando…", "Verbindung wird hergestellt…"),
    "Non connecté": ("Not connected", "Sin conexión", "Nicht verbunden"),
    "Se connecter": ("Connect", "Conectar", "Verbinden"),
    "Déconnecter": ("Disconnect", "Desconectar", "Trennen"),
    "Identifiants…": ("Sign-in info…", "Credenciales…", "Anmeldeinformationen…"),
    "Supprimer": ("Remove", "Quitar", "Entfernen"),
    "Annuler": ("Cancel", "Cancelar", "Abbrechen"),
    "Enregistrer": ("Save", "Guardar", "Speichern"),
    "Serveur : {server}": ("Server: {server}", "Servidor: {server}", "Server: {server}"),
    "Nom de la connexion": ("Connection name", "Nombre de la conexión", "Verbindungsname"),
    "Fichier de configuration": ("Configuration file", "Archivo de configuración", "Konfigurationsdatei"),
    "Aucun fichier choisi": ("No file chosen", "Ningún archivo elegido", "Keine Datei ausgewählt"),
    "Parcourir…": ("Browse…", "Examinar…", "Durchsuchen…"),
    "Nom d’utilisateur": ("User name", "Nombre de usuario", "Benutzername"),
    "Mot de passe": ("Password", "Contraseña", "Kennwort"),
    "Mot de passe de la clé privée": ("Private key password", "Contraseña de la clave privada", "Kennwort des privaten Schlüssels"),
    "Afficher les caractères": ("Show characters", "Mostrar caracteres", "Zeichen anzeigen"),
    "WireGuard (fichier .conf) ou OpenVPN (fichier .ovpn)": ("WireGuard (.conf file) or OpenVPN (.ovpn file)", "WireGuard (archivo .conf) u OpenVPN (archivo .ovpn)", "WireGuard (.conf-Datei) oder OpenVPN (.ovpn-Datei)"),
    "WireGuard (fichier .conf)": ("WireGuard (.conf file)", "WireGuard (archivo .conf)", "WireGuard (.conf-Datei)"),
    "OpenVPN (fichier .ovpn)": ("OpenVPN (.ovpn file)", "OpenVPN (archivo .ovpn)", "OpenVPN (.ovpn-Datei)"),
    "Ce serveur demande un nom d’utilisateur et un mot de passe.": ("This server asks for a user name and password.", "Este servidor pide un nombre de usuario y una contraseña.", "Dieser Server verlangt einen Benutzernamen und ein Kennwort."),
    "Supprimer la connexion « {name} » ?": ("Remove the “{name}” connection?", "¿Quitar la conexión «{name}»?", "Verbindung „{name}“ entfernen?"),
    "Ses réglages et ses certificats seront effacés de cet ordinateur.": ("Its settings and certificates will be erased from this computer.", "Su configuración y sus certificados se borrarán de este equipo.", "Ihre Einstellungen und Zertifikate werden von diesem Computer gelöscht."),
    "Connecté à « {name} ».": ("Connected to “{name}”.", "Conectado a «{name}».", "Verbunden mit „{name}“."),
    "Connexion à « {name} »…": ("Connecting to “{name}”…", "Conectando a «{name}»…", "Verbindung mit „{name}“ wird hergestellt…"),
    "Connexion « {name} » ajoutée.": ("“{name}” connection added.", "Conexión «{name}» agregada.", "Verbindung „{name}“ hinzugefügt."),
    "Le VPN « {name} » est actif : le trafic passe par son serveur.": ("The “{name}” VPN is on: traffic goes through its server.", "La VPN «{name}» está activa: el tráfico pasa por su servidor.", "Das VPN „{name}“ ist aktiv: Der Datenverkehr läuft über seinen Server."),
    "Détail : {detail}": ("Details: {detail}", "Detalle: {detail}", "Details: {detail}"),
    "La connexion a échoué.": ("The connection failed.", "La conexión ha fallado.", "Die Verbindung ist fehlgeschlagen."),
    # Erreurs
    "Ce fichier n’est pas une configuration WireGuard valable (sections [Interface] et [Peer], clés, adresses).":
        ("This file is not a valid WireGuard configuration ([Interface] and [Peer] sections, keys, addresses).",
         "Este archivo no es una configuración WireGuard válida (secciones [Interface] y [Peer], claves, direcciones).",
         "Diese Datei ist keine gültige WireGuard-Konfiguration (Abschnitte [Interface] und [Peer], Schlüssel, Adressen)."),
    "Ce fichier n’est pas une configuration OpenVPN que NetworkManager sait lire.":
        ("This file is not an OpenVPN configuration NetworkManager can read.",
         "Este archivo no es una configuración OpenVPN que NetworkManager pueda leer.",
         "Diese Datei ist keine OpenVPN-Konfiguration, die NetworkManager lesen kann."),
    "Choisissez un fichier .conf (WireGuard) ou .ovpn (OpenVPN).": ("Choose a .conf (WireGuard) or .ovpn (OpenVPN) file.", "Elija un archivo .conf (WireGuard) u .ovpn (OpenVPN).", "Wählen Sie eine .conf- (WireGuard) oder .ovpn-Datei (OpenVPN)."),
    "Ce type de VPN n’est pas pris en charge sur cet ordinateur.": ("This kind of VPN is not supported on this computer.", "Este tipo de VPN no es compatible con este equipo.", "Dieser VPN-Typ wird auf diesem Computer nicht unterstützt."),
    "Le fichier « {name} » cité par la configuration est introuvable ou illisible. Placez-le à côté du fichier de configuration.":
        ("The file “{name}” named in the configuration cannot be found or read. Put it next to the configuration file.",
         "No se encuentra o no se puede leer el archivo «{name}» citado en la configuración. Colóquelo junto al archivo de configuración.",
         "Die in der Konfiguration genannte Datei „{name}“ fehlt oder ist nicht lesbar. Legen Sie sie neben die Konfigurationsdatei."),
    "Le fichier de configuration est trop grand.": ("The configuration file is too large.", "El archivo de configuración es demasiado grande.", "Die Konfigurationsdatei ist zu groß."),
    "Donnez un nom à la connexion.": ("Give the connection a name.", "Dé un nombre a la conexión.", "Geben Sie der Verbindung einen Namen."),
    "Le serveur a refusé le nom d’utilisateur ou le mot de passe. Corrigez-les dans « Identifiants… ».":
        ("The server refused the user name or password. Correct them in “Sign-in info…”.",
         "El servidor rechazó el nombre de usuario o la contraseña. Corríjalos en «Credenciales…».",
         "Der Server hat Benutzername oder Kennwort abgelehnt. Korrigieren Sie sie unter „Anmeldeinformationen…“."),
    "Le serveur VPN n’a pas répondu à temps. Vérifiez votre connexion à Internet, puis réessayez.":
        ("The VPN server did not answer in time. Check your Internet connection, then try again.",
         "El servidor VPN no respondió a tiempo. Compruebe su conexión a Internet y vuelva a intentarlo.",
         "Der VPN-Server hat nicht rechtzeitig geantwortet. Prüfen Sie Ihre Internetverbindung und versuchen Sie es erneut."),
    "Le greffon OpenVPN de NetworkManager est absent de ce système.": ("NetworkManager's OpenVPN plug-in is missing from this system.", "Falta el complemento OpenVPN de NetworkManager en este sistema.", "Das OpenVPN-Plug-in von NetworkManager fehlt auf diesem System."),
    "Le service réseau (NetworkManager) ne répond pas. Redémarrez l’ordinateur.": ("The network service (NetworkManager) is not responding. Restart the computer.", "El servicio de red (NetworkManager) no responde. Reinicie el equipo.", "Der Netzwerkdienst (NetworkManager) antwortet nicht. Starten Sie den Computer neu."),
    "Le service système de Winlux ne répond pas. Redémarrez l’ordinateur.": ("The Winlux system service is not responding. Restart the computer.", "El servicio del sistema de Winlux no responde. Reinicie el equipo.", "Der Winlux-Systemdienst antwortet nicht. Starten Sie den Computer neu."),
    "Cette connexion n’existe plus.": ("This connection no longer exists.", "Esta conexión ya no existe.", "Diese Verbindung existiert nicht mehr."),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# ── Service (root) : NetworkManager ─────────────────────────────────────────

def nm(args, timeout=20, env=None):
    """nmcli en anglais (messages analysés), sans jamais rester bloqué."""
    full = dict(os.environ, LC_ALL="C", LANG="C", LANGUAGE="C", **(env or {}))
    try:
        return subprocess.run(["nmcli", *args], capture_output=True, text=True, timeout=timeout, env=full)
    except FileNotFoundError:
        return subprocess.CompletedProcess(args, 127, "", "NetworkManager is not running (nmcli missing)")
    except subprocess.TimeoutExpired:
        return subprocess.CompletedProcess(args, 124, "", "Timeout expired")


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


def classify(message):
    text = (message or "").lower()
    rules = (
        (("not authorized", "insufficient privileges", "permission denied"), "not-authorized"),
        (("is not running", "could not create nmclient", "nmcli missing"), "no-service"),
        (("secrets were required", "no secrets", "no valid secrets", "auth-failed", "authentication failed",
          "login-failed", "auth_failed"), "credentials"),
        (("plugin", "service-type", "unknown vpn", "vpn service"), "no-plugin"),
        (("unknown connection", "no such connection", "not found"), "unknown"),
        (("timeout", "timed out", "connect-timeout", "service-start-timeout"), "timeout"),
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


def _exists(relative):
    try:
        return (SYS_ROOT / relative).exists()
    except OSError:
        return False


def wireguard_supported():
    """Module wireguard du noyau chargé ou livré avec le noyau en marche."""
    if _exists("sys/module/wireguard"):
        return True
    release = os.environ.get("WINLUX_KERNEL_RELEASE") or os.uname().release
    for base in ("lib/modules", "usr/lib/modules"):
        directory = SYS_ROOT / base / release
        try:
            if any(directory.glob("kernel/drivers/net/wireguard/wireguard.ko*")):
                return True
        except OSError:
            pass
    return False


def openvpn_supported():
    """Greffon OpenVPN de NetworkManager et programme openvpn installés."""
    plugin = any(_exists(f"{base}/NetworkManager/VPN/nm-openvpn-service.name")
                 for base in ("usr/lib", "etc", "usr/lib/x86_64-linux-gnu"))
    return plugin and any(_exists(path) for path in ("usr/sbin/openvpn", "sbin/openvpn", "usr/bin/openvpn"))


def kinds():
    found = []
    if wireguard_supported(): found.append("wireguard")
    if openvpn_supported(): found.append("openvpn")
    return found


def available():
    """La page n'existe qu'avec NetworkManager et au moins un type de VPN."""
    return shutil.which("nmcli") is not None and bool(kinds())


# ── Fichiers de profil (GKeyFile) ───────────────────────────────────────────

def keyfile_escape(value):
    value = value.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t")
    return "\\s" + value[1:] if value.startswith(" ") else value


def read_keyfile(path):
    """[[section, [[clé, valeur], …]], …] dans l'ordre du fichier."""
    sections, current = [], None
    for line in Path(path).read_text(encoding="utf-8", errors="surrogateescape").splitlines():
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            current = [stripped[1:-1], []]
            sections.append(current)
        elif current is not None and "=" in line and not stripped.startswith("#"):
            key, value = line.split("=", 1)
            current[1].append([key.strip(), value.strip()])
    return sections


def keyfile_get(sections, section, key, default=""):
    for name, values in sections:
        if name == section:
            for k, v in values:
                if k == key: return v
    return default


def keyfile_set(sections, section, key, value):
    for name, values in sections:
        if name == section:
            for pair in values:
                if pair[0] == key:
                    pair[1] = value
                    return
            values.append([key, value])
            return
    sections.append([section, [[key, value]]])


def keyfile_delete(sections, section, key):
    for name, values in sections:
        if name == section:
            values[:] = [pair for pair in values if pair[0] != key]


def keyfile_text(sections):
    return "\n".join(f"[{name}]\n" + "".join(f"{k}={v}\n" for k, v in values) for name, values in sections)


def write_private(path, text):
    """Écrit un fichier 0600 (profil système) de façon atomique."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name("." + path.name + ".tmp")
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8", errors="surrogateescape") as handle:
        handle.write(text)
    os.chmod(temporary, 0o600)
    os.replace(temporary, path)
    return path


# ── WireGuard : fichier .conf → profil NetworkManager ───────────────────────

def parse_wireguard(text):
    """Configuration wg-quick -> {"interface": {...}, "peers": [...]} ; ValueError si invalide."""
    interface, peers, section = {}, [], None
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        if line.startswith("["):
            name = line.strip("[] ").lower()
            if name == "interface":
                section = interface
            elif name == "peer":
                section = {}
                peers.append(section)
            else:
                section = None
            continue
        if section is None or "=" not in line:
            continue
        key, value = (part.strip() for part in line.split("=", 1))
        key = key.lower()
        if key in ("address", "dns", "allowedips"):
            section.setdefault(key, []).extend(item.strip() for item in value.split(",") if item.strip())
        else:
            section[key] = value
    if not WG_KEY.match(interface.get("privatekey", "")) or not peers:
        raise ValueError("wg-format")
    addresses = []
    for item in interface.get("address", []):
        try: addresses.append(ipaddress.ip_interface(item))
        except ValueError: raise ValueError("wg-format")
    if not addresses:
        raise ValueError("wg-format")
    dns, search = [], []
    for item in interface.get("dns", []):
        try: dns.append(ipaddress.ip_address(item))
        except ValueError:
            if re.fullmatch(r"[A-Za-z0-9.-]{1,253}", item): search.append(item)
            else: raise ValueError("wg-format")
    clean_peers = []
    for peer in peers:
        if not WG_KEY.match(peer.get("publickey", "")):
            raise ValueError("wg-format")
        allowed = []
        for item in peer.get("allowedips", []):
            try: allowed.append(ipaddress.ip_network(item, strict=False))
            except ValueError: raise ValueError("wg-format")
        endpoint = peer.get("endpoint", "")
        if endpoint and not re.fullmatch(r"(\[[0-9A-Fa-f:.]+\]|[A-Za-z0-9.-]{1,253}):[0-9]{1,5}", endpoint):
            raise ValueError("wg-format")
        psk = peer.get("presharedkey", "")
        if psk and not WG_KEY.match(psk):
            raise ValueError("wg-format")
        keepalive = peer.get("persistentkeepalive", "")
        if keepalive and keepalive != "off" and not keepalive.isdigit():
            raise ValueError("wg-format")
        clean_peers.append({"public": peer["publickey"], "endpoint": endpoint, "psk": psk,
                            "keepalive": "" if keepalive == "off" else keepalive, "allowed": allowed})
    result = {"private": interface["privatekey"], "addresses": addresses, "dns": dns, "search": search,
              "peers": clean_peers}
    for key, name in (("listenport", "port"), ("mtu", "mtu")):
        value = interface.get(key, "")
        if value:
            if not value.isdigit(): raise ValueError("wg-format")
            result[name] = value
    mark = interface.get("fwmark", "")
    if mark and mark != "off":
        try: result["fwmark"] = str(int(mark, 0))
        except ValueError: raise ValueError("wg-format")
    return result


def full_tunnel(config):
    return any(network.prefixlen == 0 for peer in config["peers"] for network in peer["allowed"])


def wireguard_keyfile(name, config, ident, ifname):
    lines = ["[connection]", f"id={keyfile_escape(name)}", f"uuid={ident}", "type=wireguard",
             f"interface-name={ifname}", "autoconnect=false", "", "[wireguard]",
             f"private-key={config['private']}"]
    if config.get("port"): lines.append(f"listen-port={config['port']}")
    if config.get("mtu"): lines.append(f"mtu={config['mtu']}")
    if config.get("fwmark"): lines.append(f"fwmark={config['fwmark']}")
    lines.append("")
    for peer in config["peers"]:
        lines.append(f"[wireguard-peer.{peer['public']}]")
        if peer["endpoint"]: lines.append(f"endpoint={peer['endpoint']}")
        if peer["psk"]: lines += [f"preshared-key={peer['psk']}", "preshared-key-flags=0"]
        if peer["keepalive"]: lines.append(f"persistent-keepalive={peer['keepalive']}")
        lines.append("allowed-ips=" + "".join(f"{network};" for network in peer["allowed"]))
        lines.append("")
    # Tout le trafic : DNS du VPN exclusifs (priorité négative), pas de fuite
    # vers la box. Tunnel partiel : DNS du VPN en premier seulement.
    priority = "-50" if full_tunnel(config) else "50"
    for family, version in (("ipv4", 4), ("ipv6", 6)):
        addresses = [address for address in config["addresses"] if address.version == version]
        servers = [server for server in config["dns"] if server.version == version]
        lines.append(f"[{family}]")
        if addresses:
            lines.append("method=manual")
            lines += [f"address{index}={address}" for index, address in enumerate(addresses, 1)]
            if servers:
                lines.append("dns=" + "".join(f"{server};" for server in servers))
                lines.append(f"dns-priority={priority}")
                search = list(config["search"]) + (["~"] if priority == "-50" else [])
                if search: lines.append("dns-search=" + "".join(f"{item};" for item in search))
        else:
            lines.append("method=disabled")
        lines.append("")
    return "\n".join(lines)


def used_ifnames():
    names = set()
    try: names.update(entry.name for entry in (SYS_ROOT / "sys/class/net").iterdir())
    except OSError: pass
    try:
        for path in CONNECTIONS.glob("*.nmconnection"):
            try: names.add(keyfile_get(read_keyfile(path), "connection", "interface-name"))
            except OSError: pass
    except OSError: pass
    return names


def free_ifname():
    used = used_ifnames()
    return next(f"wg{index}" for index in range(1000) if f"wg{index}" not in used)


def load(path):
    result = nm(["connection", "load", str(path)], timeout=10)
    if result.returncode:
        result = nm(["connection", "reload"], timeout=10)
    return result


def import_wireguard(name, text):
    if "wireguard" not in kinds():
        return fail("unsupported")
    try:
        config = parse_wireguard(text)
    except ValueError as error:
        return fail(str(error))
    ident = str(uuid.uuid4())
    path = CONNECTIONS / f"winlux-vpn-{ident}.nmconnection"
    try:
        write_private(path, wireguard_keyfile(name, config, ident, free_ifname()))
    except OSError as error:
        return fail("not-authorized", str(error))
    result = load(path)
    if result.returncode:
        path.unlink(missing_ok=True)
        return fail(classify(result.stderr), result.stderr)
    return done(uuid=ident, name=name, kind="wireguard")


# ── OpenVPN : import par le greffon de NetworkManager ───────────────────────

def openvpn_needs(text):
    """(identifiants demandés, clé privée chiffrée) d'après le .ovpn."""
    auth = re.search(r"^\s*auth-user-pass\b", text, re.MULTILINE) is not None
    encrypted = "ENCRYPTED PRIVATE KEY" in text or "Proc-Type: 4,ENCRYPTED" in text
    return auth, encrypted


def profile_path(ident):
    """Fichier du profil d'après NetworkManager (FILENAME), sinon par recherche."""
    result = nm(["-t", "-f", "UUID,FILENAME", "connection", "show"], timeout=10)
    for line in result.stdout.splitlines():
        fields = split_terse(line)
        if len(fields) >= 2 and fields[0] == ident and fields[1]:
            return Path(fields[1])
    for path in CONNECTIONS.glob("*.nmconnection"):
        try:
            if keyfile_get(read_keyfile(path), "connection", "uuid") == ident:
                return path
        except OSError:
            continue
    return None


def apply_credentials(sections, username="", password="", key_password=""):
    """Secrets gardés dans le profil système (pas d'agent de secrets sans session)."""
    if username:
        keyfile_set(sections, "vpn", "username", keyfile_escape(username))
    if password:
        keyfile_set(sections, "vpn", "password-flags", "0")
        keyfile_set(sections, "vpn-secrets", "password", keyfile_escape(password))
    if key_password:
        keyfile_set(sections, "vpn", "cert-pass-flags", "0")
        keyfile_set(sections, "vpn-secrets", "cert-pass", keyfile_escape(key_password))


def import_openvpn(name, text, username="", password="", key_password=""):
    if "openvpn" not in kinds():
        return fail("unsupported")
    if not re.search(r"^\s*(remote|client|dev)\b", text, re.MULTILINE):
        return fail("ovpn-format")
    token = uuid.uuid4().hex
    data_home = VPN_DATA / token
    work = Path(tempfile.mkdtemp(prefix="winlux-vpn-"))
    try:
        os.chmod(work, 0o700)
        source = work / "vpn.ovpn"
        write_private(source, text)
        data_home.mkdir(parents=True, exist_ok=True)
        os.chmod(VPN_DATA, 0o700)
        result = nm(["connection", "import", "type", "openvpn", "file", str(source)], timeout=30,
                    env={"XDG_DATA_HOME": str(data_home)})
    finally:
        shutil.rmtree(work, ignore_errors=True)
    found = re.search(r"\(([0-9a-fA-F-]{36})\)", result.stdout + result.stderr)
    if result.returncode or not found:
        shutil.rmtree(data_home, ignore_errors=True)
        message = result.stderr or result.stdout
        code = classify(message)
        return fail("ovpn-format" if code == "failed" else code, message)
    ident = found[1]
    path = profile_path(ident)
    if path is None or not path.exists():
        return done(uuid=ident, name=name, kind="openvpn", warning="no-keyfile")
    sections = read_keyfile(path)
    keyfile_set(sections, "connection", "id", keyfile_escape(name))
    keyfile_set(sections, "connection", "autoconnect", "false")
    keyfile_delete(sections, "connection", "permissions")
    apply_credentials(sections, username, password, key_password)
    write_private(path, keyfile_text(sections))
    load(path)
    if not any(data_home.iterdir()):
        data_home.rmdir()
    return done(uuid=ident, name=name, kind="openvpn")


# ── Connexions ──────────────────────────────────────────────────────────────

def connection_details(path):
    """(type, serveur) lus dans le profil (root seulement)."""
    try:
        sections = read_keyfile(path)
    except (OSError, TypeError):
        return "", ""
    if keyfile_get(sections, "connection", "type") == "wireguard":
        for name, values in sections:
            if name.startswith("wireguard-peer."):
                return "wireguard", dict(values).get("endpoint", "")
        return "wireguard", ""
    service = keyfile_get(sections, "vpn", "service-type")
    remote = keyfile_get(sections, "vpn", "remote").split(",")[0].strip()
    return ("openvpn" if "openvpn" in service else "vpn"), remote


def connections():
    result = nm(["-t", "-f", "NAME,UUID,TYPE,ACTIVE,STATE,TIMESTAMP,FILENAME", "connection", "show"], timeout=10)
    found = []
    for line in result.stdout.splitlines():
        fields = split_terse(line)
        if len(fields) < 7 or fields[2] not in ("wireguard", "vpn"):
            continue
        name, ident, kind, active, state, stamp, filename = fields[:7]
        detail_kind, server = connection_details(filename) if filename else ("", "")
        found.append({"name": name, "uuid": ident, "kind": detail_kind or ("wireguard" if kind == "wireguard" else "vpn"),
                      "active": active == "yes" and state in ("activated", ""),
                      "activating": active == "yes" and state == "activating",
                      "server": server, "timestamp": int(stamp) if stamp.isdigit() else 0})
    return found


def find(ident):
    return next((item for item in connections() if item["uuid"] == ident), None)


def fix_resolv():
    """resolv.conf réécrit par NetworkManager (umask 077) : lisible tout de suite."""
    if os.access(PERMS, os.X_OK):
        try: subprocess.run([PERMS], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15)
        except (OSError, subprocess.SubprocessError): pass


def connect(ident):
    item = find(ident)
    if item is None:
        return fail("unknown")
    result = nm(["-w", str(CONNECT_WAIT), "connection", "up", "uuid", ident], timeout=CONNECT_WAIT + 15)
    fix_resolv()
    if result.returncode:
        message = (result.stderr or "") + (result.stdout or "")
        return fail(classify(message), message, name=item["name"])
    return done(name=item["name"])


def disconnect(ident):
    result = nm(["connection", "down", "uuid", ident], timeout=20)
    fix_resolv()
    message = (result.stderr or "") + (result.stdout or "")
    if result.returncode and "not an active connection" not in message.lower():
        return fail(classify(message), message)
    return done()


def credentials(ident, payload):
    path = profile_path(ident)
    if path is None or not path.exists():
        return fail("unknown")
    sections = read_keyfile(path)
    apply_credentials(sections, payload.get("username", ""), payload.get("password", ""),
                      payload.get("key_password", ""))
    write_private(path, keyfile_text(sections))
    result = load(path)
    return fail(classify(result.stderr), result.stderr) if result.returncode else done()


def remove(ident):
    path = profile_path(ident)
    folders = set()
    if path is not None and path.exists():
        try:
            for _, values in read_keyfile(path):
                for _, value in values:
                    if value.startswith(str(VPN_DATA) + "/"):
                        relative = Path(value).relative_to(VPN_DATA)
                        if relative.parts: folders.add(VPN_DATA / relative.parts[0])
        except (OSError, ValueError):
            pass
    nm(["connection", "down", "uuid", ident], timeout=20)
    result = nm(["connection", "delete", "uuid", ident], timeout=15)
    fix_resolv()
    if result.returncode:
        return fail(classify(result.stderr), result.stderr)
    for folder in folders:
        shutil.rmtree(folder, ignore_errors=True)
    return done()


def status():
    running = nm(["-t", "-f", "RUNNING", "general"], timeout=5).stdout.strip() == "running"
    return done(nm=running, kinds=kinds(), connections=connections() if running else [])


def read_payload():
    data = sys.stdin.read(MAX_CONFIG + 4096)
    if len(data) > MAX_CONFIG:
        raise ValueError("too-big")
    payload = json.loads(data or "{}")
    if not isinstance(payload, dict):
        raise ValueError("failed")
    return {key: value for key, value in payload.items() if isinstance(value, str)}


def clean_name(name):
    name = " ".join((name or "").split())
    return name[:64] if name and name.isprintable() else ""


def backend_main(argv):
    if not argv:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    command, rest = argv[0], argv[1:]
    if command == "status" and not rest:
        return status()
    if command == "import" and rest == ["--stdin"]:
        try: payload = read_payload()
        except ValueError as error: return fail(str(error) if str(error) == "too-big" else "failed")
        name, kind, text = clean_name(payload.get("name")), payload.get("kind"), payload.get("config", "")
        if not name: return fail("name")
        if kind == "wireguard": return import_wireguard(name, text)
        if kind == "openvpn":
            return import_openvpn(name, text, payload.get("username", ""), payload.get("password", ""),
                                  payload.get("key_password", ""))
        return fail("kind")
    if command in ("connect", "disconnect", "remove") and len(rest) == 1 and UUID.match(rest[0]):
        return {"connect": connect, "disconnect": disconnect, "remove": remove}[command](rest[0])
    if command == "credentials" and len(rest) == 2 and UUID.match(rest[0]) and rest[1] == "--stdin":
        try: payload = read_payload()
        except ValueError: return fail("failed")
        return credentials(rest[0], payload)
    print(__doc__.strip(), file=sys.stderr)
    return 2


# ── Côté bureau : appel du service, en root par le relais ───────────────────

def relayed():
    """Vrai quand ce programme doit passer par winlux-root (compte du bureau)."""
    return os.environ.get("WINLUX_VPN_DIRECT") != "1" and os.geteuid() != 0 and os.path.exists(RELAY_SOCKET)


def backend_command(args):
    if not relayed():
        return [sys.executable, "-B", os.path.abspath(__file__), *args]
    return [shutil.which("winlux-root") or "/usr/bin/winlux-root", "exec", "winlux-vpn", *args]


def call(args, payload=None, timeout=CONNECT_WAIT + 40):
    """Lance le service ; renvoie son JSON (dict), avec ok=False et error en cas d'échec."""
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
    "wg-format": "Ce fichier n’est pas une configuration WireGuard valable (sections [Interface] et [Peer], clés, adresses).",
    "ovpn-format": "Ce fichier n’est pas une configuration OpenVPN que NetworkManager sait lire.",
    "kind": "Choisissez un fichier .conf (WireGuard) ou .ovpn (OpenVPN).",
    "unsupported": "Ce type de VPN n’est pas pris en charge sur cet ordinateur.",
    "too-big": "Le fichier de configuration est trop grand.",
    "name": "Donnez un nom à la connexion.",
    "credentials": "Le serveur a refusé le nom d’utilisateur ou le mot de passe. Corrigez-les dans « Identifiants… ».",
    "timeout": "Le serveur VPN n’a pas répondu à temps. Vérifiez votre connexion à Internet, puis réessayez.",
    "no-plugin": "Le greffon OpenVPN de NetworkManager est absent de ce système.",
    "no-service": "Le service réseau (NetworkManager) ne répond pas. Redémarrez l’ordinateur.",
    "relay": "Le service système de Winlux ne répond pas. Redémarrez l’ordinateur.",
    "not-authorized": "Le service système de Winlux ne répond pas. Redémarrez l’ordinateur.",
    "unknown": "Cette connexion n’existe plus.",
}


def error_text(answer):
    code = answer.get("error", "failed")
    if code == "missing-file":
        return _t("Le fichier « {name} » cité par la configuration est introuvable ou illisible. Placez-le à côté du fichier de configuration.").format(name=answer.get("detail", ""))
    text = _t(ERRORS.get(code, "La connexion a échoué."))
    detail = answer.get("detail", "")
    if code not in ERRORS and detail:
        text += "\n" + _t("Détail : {detail}").format(detail=detail.splitlines()[-1][:200])
    return text


# Directives d'OpenVPN qui citent un fichier : incluses dans le .ovpn par la
# page (avec les droits du compte) pour que le profil ne dépende plus du dossier
# d'origine. « [inline] » : déjà inclus.
OVPN_FILES = ("ca", "cert", "key", "tls-auth", "tls-crypt", "tls-crypt-v2", "pkcs12", "secret",
              "extra-certs", "dh", "crl-verify")


def inline_openvpn(path):
    """(texte, nom d'utilisateur lu dans auth-user-pass FICHIER) ; FileNotFoundError(nom) sinon."""
    path = Path(path)
    text = path.read_text(encoding="utf-8", errors="replace")
    out, username, password = [], "", ""
    for line in text.splitlines():
        words = line.strip().split()
        directive = words[0].lower() if words else ""
        argument = words[1].strip("\"'") if len(words) > 1 else ""
        if directive == "auth-user-pass":
            if argument:
                try:
                    lines = (path.parent / argument).read_text(encoding="utf-8").splitlines()
                    username = lines[0].strip() if lines else ""
                    password = lines[1].strip() if len(lines) > 1 else ""
                except OSError:
                    pass
            out.append("auth-user-pass")
            continue
        if directive in OVPN_FILES and argument and argument != "[inline]":
            if directive == "crl-verify" and len(words) > 2 and words[2] == "dir":
                out.append(line)
                continue
            target = path.parent / argument
            try:
                if not target.is_file() or target.stat().st_size > MAX_CONFIG: raise OSError
                raw = target.read_bytes()
            except OSError:
                raise FileNotFoundError(argument)
            body = base64.encodebytes(raw).decode() if directive == "pkcs12" else raw.decode("utf-8", "replace")
            out += [f"<{directive}>", body.rstrip("\n"), f"</{directive}>"]
            if directive in ("tls-auth", "secret") and len(words) > 2 and words[2] in ("0", "1"):
                out.append(f"key-direction {words[2]}")
            continue
        out.append(line)
    return "\n".join(out) + "\n", username, password


def config_kind(path, text):
    if re.search(r"^\s*\[Interface\]", text, re.MULTILINE | re.IGNORECASE):
        return "wireguard"
    if str(path).lower().endswith((".ovpn", ".opvn")) or re.search(r"^\s*(remote|client)\b", text, re.MULTILINE):
        return "openvpn"
    return ""


def page(host):
    return VpnPanel(host)


class VpnPanel:
    """Page VPN de Paramètres, construite dans `host` (section, card, row, hint…)."""

    def __init__(self, host):
        import gi
        gi.require_version("Gtk", "3.0")
        from gi.repository import GLib, Gtk
        self.Gtk, self.GLib = Gtk, GLib
        self.host = host
        self.busy = False
        self.items = []
        self.kinds = kinds()
        intro = host.card()
        host.hint(_t("Un VPN fait passer le trafic de cet ordinateur par un serveur choisi : celui de votre entreprise, de votre école ou d’un fournisseur. Ajoutez le fichier de configuration qu’il vous a donné."), intro)
        add = Gtk.Button(label=_t("Ajouter un VPN"))
        add.set_name("vpn-add")
        add.get_style_context().add_class("suggested-action")
        add.connect("clicked", lambda *_: self.add_dialog())
        host.row(intro, _t(self.kinds_label()), add)
        host.section(_t("Connexions VPN"))
        self.card = host.card()
        self.message = Gtk.Label(xalign=0, wrap=True, selectable=True)
        self.message.set_name("vpn-message")
        self.message.set_no_show_all(True)
        self.card.pack_start(self.message, False, False, 0)
        self.list_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.card.pack_start(self.list_box, False, False, 0)
        GLib.idle_add(lambda: self.refresh() and False)
        GLib.timeout_add_seconds(5, self.poll)

    def kinds_label(self):
        if self.kinds == ["wireguard"]: return "WireGuard (fichier .conf)"
        if self.kinds == ["openvpn"]: return "OpenVPN (fichier .ovpn)"
        return "WireGuard (fichier .conf) ou OpenVPN (fichier .ovpn)"

    def poll(self):
        if self.card.get_toplevel() is None or not self.card.get_realized():
            return self.card.get_parent() is not None
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

    def got_status(self, answer):
        if not answer.get("ok"):
            self.say(error_text(answer), True)
            return
        if answer.get("kinds"): self.kinds = answer["kinds"]
        self.items = answer.get("connections", [])
        self.show()

    def subtitle(self, item):
        parts = [{"wireguard": "WireGuard", "openvpn": "OpenVPN"}.get(item.get("kind"), "VPN")]
        if item.get("server"): parts.append(_t("Serveur : {server}").format(server=item["server"]))
        parts.append(_t("Connecté") if item.get("active") else _t("Connexion…") if item.get("activating")
                     else _t("Non connecté"))
        return " · ".join(parts)

    def show(self):
        Gtk = self.Gtk
        for child in self.list_box.get_children(): child.destroy()
        if not self.items:
            self.host.hint(_t("Aucune connexion VPN."), self.list_box)
        for item in self.items:
            buttons = Gtk.Box(spacing=8)
            if item.get("active") or item.get("activating"):
                main = Gtk.Button(label=_t("Déconnecter"))
                main.connect("clicked", lambda *_, i=item: self.disconnect(i))
            else:
                main = Gtk.Button(label=_t("Se connecter"))
                main.get_style_context().add_class("suggested-action")
                main.connect("clicked", lambda *_, i=item: self.connect(i))
            main.get_accessible().set_name(f"{main.get_label()} {item['name']}")
            buttons.pack_start(main, False, False, 0)
            if item.get("kind") == "openvpn":
                creds = Gtk.Button(label=_t("Identifiants…"))
                creds.connect("clicked", lambda *_, i=item: self.credentials_dialog(i))
                buttons.pack_start(creds, False, False, 0)
            remove = Gtk.Button(label=_t("Supprimer"))
            remove.connect("clicked", lambda *_, i=item: self.remove(i))
            buttons.pack_start(remove, False, False, 0)
            line = self.host.row(self.list_box, item["name"], buttons, self.subtitle(item))
            image = Gtk.Image.new_from_icon_name("network-vpn-symbolic", Gtk.IconSize.LARGE_TOOLBAR)
            line.pack_start(image, False, False, 0)
            line.reorder_child(image, 0)
        self.list_box.show_all()

    def after(self, answer, success=""):
        if answer.get("ok"):
            self.say(success)
        else:
            self.say(error_text(answer), True)
        self.refresh()

    def connect(self, item):
        self.say(_t("Connexion à « {name} »…").format(name=item["name"]))
        self.run(["connect", item["uuid"]],
                 lambda answer: self.after(answer, _t("Le VPN « {name} » est actif : le trafic passe par son serveur.").format(name=item["name"])))

    def disconnect(self, item):
        self.run(["disconnect", item["uuid"]], self.after)

    def remove(self, item):
        Gtk = self.Gtk
        dialog = Gtk.MessageDialog(transient_for=self.host, modal=True, message_type=Gtk.MessageType.QUESTION,
                                   buttons=Gtk.ButtonsType.NONE,
                                   text=_t("Supprimer la connexion « {name} » ?").format(name=item["name"]))
        dialog.format_secondary_text(_t("Ses réglages et ses certificats seront effacés de cet ordinateur."))
        dialog.add_button(_t("Annuler"), Gtk.ResponseType.CANCEL)
        dialog.add_button(_t("Supprimer"), Gtk.ResponseType.OK)
        answer = dialog.run()
        dialog.destroy()
        if answer == Gtk.ResponseType.OK:
            self.run(["remove", item["uuid"]], self.after)

    # ── Boîtes de dialogue ──

    def dialog(self, title, ok_label):
        Gtk = self.Gtk
        dialog = Gtk.Dialog(title=title, transient_for=self.host, modal=True)
        dialog.set_role("winlux-vpn")
        dialog.add_button(_t("Annuler"), Gtk.ResponseType.CANCEL)
        ok = dialog.add_button(ok_label, Gtk.ResponseType.OK)
        ok.get_style_context().add_class("suggested-action")
        dialog.set_default_response(Gtk.ResponseType.OK)
        screen = self.host.get_screen()
        width = screen.get_width() if screen is not None else 1366
        dialog.set_default_size(min(560, max(340, width - 32)), -1)
        area = dialog.get_content_area()
        area.set_spacing(8)
        area.set_border_width(16)
        return dialog, area

    def field(self, parent, title, secret=False):
        Gtk = self.Gtk
        parent.pack_start(Gtk.Label(label=title, xalign=0), False, False, 0)
        entry = Gtk.Entry(visibility=not secret, activates_default=True)
        if secret: entry.set_input_purpose(Gtk.InputPurpose.PASSWORD)
        parent.pack_start(entry, False, False, 0)
        return entry

    def secret_fields(self, area):
        Gtk = self.Gtk
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        area.pack_start(box, False, False, 0)
        notice = Gtk.Label(label=_t("Ce serveur demande un nom d’utilisateur et un mot de passe."), xalign=0, wrap=True)
        box.pack_start(notice, False, False, 0)
        user = self.field(box, _t("Nom d’utilisateur"))
        user.set_name("vpn-user")
        password = self.field(box, _t("Mot de passe"), secret=True)
        password.set_name("vpn-password")
        key_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        box.pack_start(key_box, False, False, 0)
        key_password = self.field(key_box, _t("Mot de passe de la clé privée"), secret=True)
        show = Gtk.CheckButton(label=_t("Afficher les caractères"))
        show.connect("toggled", lambda button: (password.set_visibility(button.get_active()),
                                                key_password.set_visibility(button.get_active())))
        box.pack_start(show, False, False, 0)
        return box, key_box, user, password, key_password

    def add_dialog(self):
        Gtk = self.Gtk
        dialog, area = self.dialog(_t("Ajouter un VPN"), _t("Enregistrer"))
        name = self.field(area, _t("Nom de la connexion"))
        name.set_name("vpn-name")
        area.pack_start(Gtk.Label(label=_t("Fichier de configuration"), xalign=0), False, False, 0)
        file_row = Gtk.Box(spacing=8)
        chosen_label = Gtk.Label(label=_t("Aucun fichier choisi"), xalign=0, ellipsize=3)
        chosen_label.set_name("vpn-file")
        file_row.pack_start(chosen_label, True, True, 0)
        browse = Gtk.Button(label=_t("Parcourir…"))
        file_row.pack_start(browse, False, False, 0)
        area.pack_start(file_row, False, False, 0)
        area.pack_start(Gtk.Label(label=_t(self.kinds_label()), xalign=0, wrap=True), False, False, 0)
        secrets, key_box, user, password, key_password = self.secret_fields(area)
        secrets.set_no_show_all(True)
        problem = Gtk.Label(xalign=0, wrap=True)
        problem.get_style_context().add_class("error")
        problem.set_name("vpn-error")
        problem.set_no_show_all(True)
        area.pack_start(problem, False, False, 0)
        state = {"path": "", "text": "", "kind": ""}

        def choose_file(path):
            problem.hide()
            state.update(path=path, text="", kind="")
            chosen_label.set_text(Path(path).name)
            try:
                if Path(path).stat().st_size > MAX_CONFIG: raise ValueError("too-big")
                raw = Path(path).read_text(encoding="utf-8", errors="replace")
                kind = config_kind(path, raw)
                if kind not in self.kinds: raise ValueError("unsupported" if kind else "kind")
                if kind == "openvpn":
                    text, found_user, found_password = inline_openvpn(path)
                    if found_user and not user.get_text(): user.set_text(found_user)
                    if found_password and not password.get_text(): password.set_text(found_password)
                else:
                    text = raw
                state.update(text=text, kind=kind)
            except FileNotFoundError as error:
                show_problem(error_text({"error": "missing-file", "detail": str(error.args[0] if error.args else "")}))
            except (OSError, ValueError) as error:
                show_problem(error_text({"error": str(error) if str(error) in ERRORS else "kind"}))
            if not name.get_text().strip():
                name.set_text(Path(path).stem.replace("_", " ")[:64])
            update()

        def show_problem(text):
            problem.set_text(text)
            problem.show()

        def update():
            needs_auth, encrypted = openvpn_needs(state["text"]) if state["kind"] == "openvpn" else (False, False)
            if needs_auth or encrypted:
                secrets.show_all()
            else:
                secrets.hide()
            for child in secrets.get_children()[:5]: child.set_visible(needs_auth)
            key_box.set_visible(encrypted)
            # La boîte reprend la hauteur de ce qui est affiché.
            dialog.resize(max(1, dialog.get_allocated_width()), 1)

        filters = Gtk.FileFilter()
        filters.set_name(_t(self.kinds_label()))
        for pattern in ("*.conf", "*.ovpn", "*.CONF", "*.OVPN"):
            filters.add_pattern(pattern)

        def pick(*_):
            chooser = Gtk.FileChooserNative.new(_t("Fichier de configuration"), dialog, Gtk.FileChooserAction.OPEN, None, None)
            chooser.add_filter(filters)
            if chooser.run() == Gtk.ResponseType.ACCEPT and chooser.get_filename():
                choose_file(chooser.get_filename())
            chooser.destroy()
        browse.connect("clicked", pick)
        dialog.choose_file = choose_file
        self.add_window = dialog
        dialog.show_all()
        update()
        while True:
            response = dialog.run()
            if response != Gtk.ResponseType.OK:
                break
            if not name.get_text().strip():
                show_problem(_t(ERRORS["name"]))
                continue
            if not state["kind"]:
                if not problem.get_visible(): show_problem(_t(ERRORS["kind"]))
                continue
            payload = {"name": name.get_text().strip(), "kind": state["kind"], "config": state["text"],
                       "username": user.get_text().strip(), "password": password.get_text(),
                       "key_password": key_password.get_text()}
            self.say("")
            self.run(["import", "--stdin"],
                     lambda answer, n=payload["name"]: self.after(answer, _t("Connexion « {name} » ajoutée.").format(name=n)),
                     payload)
            break
        self.add_window = None
        dialog.destroy()

    def credentials_dialog(self, item):
        Gtk = self.Gtk
        dialog, area = self.dialog(item["name"], _t("Enregistrer"))
        _, key_box, user, password, key_password = self.secret_fields(area)
        dialog.show_all()
        if dialog.run() == Gtk.ResponseType.OK:
            payload = {"username": user.get_text().strip(), "password": password.get_text(),
                       "key_password": key_password.get_text()}
            self.run(["credentials", item["uuid"], "--stdin"], self.after, payload)
        dialog.destroy()


def main(argv):
    # Lancé par un compte du bureau (Réglages rapides, ligne de commande) :
    # le travail se fait en root, par le relais, entrée standard comprise.
    if argv and argv[0] != "--help" and relayed():
        tool = shutil.which("winlux-root") or "/usr/bin/winlux-root"
        os.execv(tool, [tool, "exec", "winlux-vpn", *argv])
    return backend_main(argv)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
