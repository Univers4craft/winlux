#!/usr/bin/python3
"""Migration Winlux depuis un autre PC du réseau local.

Découverte : mDNS/DNS-SD maison (requêtes « legacy unicast », RFC 6762 §6.7),
compatible avec Avahi et avec `winlux-migrate --serve`. Transfert : rsync sur SSH
depuis Linux/Winlux, scp (SFTP) depuis Windows et son serveur OpenSSH.

  winlux-migrate                  assistant graphique (vers son propre dossier)
  winlux-migrate --scan           lister les PC trouvés
  winlux-migrate --serve          rendre ce PC visible (démarre sshd le temps du transfert)
"""
import argparse
import ipaddress
import json
import os
import re
import select
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time


# ── Langue : celle choisie dans l'installateur (WINLUX_LANG), sinon celle du système ──

_COL = {"en": 0, "de": 1, "es": 2, "it": 3, "pt": 4}

# Texte français -> (anglais, allemand, espagnol, italien, portugais).
_TR = {
    'Bureau': ('Desktop', 'Desktop', 'Escritorio', 'Desktop', 'Área de trabalho'),
    'Documents': ('Documents', 'Dokumente', 'Documentos', 'Documenti', 'Documentos'),
    'Téléchargements': ('Downloads', 'Downloads', 'Descargas', 'Download', 'Downloads'),
    'Images': ('Pictures', 'Bilder', 'Imágenes', 'Immagini', 'Imagens'),
    'Musique': ('Music', 'Musik', 'Música', 'Musica', 'Músicas'),
    'Vidéos': ('Videos', 'Videos', 'Vídeos', 'Video', 'Vídeos'),
    'Adresse invalide (nom de machine ou adresse IP attendue).': ('Invalid address (computer name or IP address expected).', 'Ungültige Adresse (Computername oder IP-Adresse erwartet).', 'Dirección no válida (se espera un nombre de equipo o una dirección IP).', 'Indirizzo non valido (nome del computer o indirizzo IP previsto).', 'Endereço inválido (nome do computador ou endereço IP esperado).'),
    "Nom d'utilisateur invalide.": ('Invalid user name.', 'Ungültiger Benutzername.', 'Nombre de usuario no válido.', 'Nome utente non valido.', 'Nome de usuário inválido.'),
    'Port invalide.': ('Invalid port.', 'Ungültiger Port.', 'Puerto no válido.', 'Porta non valida.', 'Porta inválida.'),
    "scp (rsync absent sur l'autre PC)": ('scp (rsync missing on the other PC)', 'scp (rsync fehlt auf dem anderen PC)', 'scp (rsync no está en el otro PC)', "scp (rsync assente sull'altro PC)", 'scp (rsync ausente no outro PC)'),
    'Connecté : PC Linux/Winlux, transfert par {tool}.': ('Connected: Linux/Winlux PC, transfer with {tool}.', 'Verbunden: Linux/Winlux-PC, Übertragung per {tool}.', 'Conectado: PC Linux/Winlux, transferencia con {tool}.', 'Connesso: PC Linux/Winlux, trasferimento con {tool}.', 'Conectado: PC Linux/Winlux, transferência por {tool}.'),
    'Connecté : PC Windows (serveur OpenSSH), transfert par scp.': ('Connected: Windows PC (OpenSSH server), transfer with scp.', 'Verbunden: Windows-PC (OpenSSH-Server), Übertragung per scp.', 'Conectado: PC Windows (servidor OpenSSH), transferencia con scp.', 'Connesso: PC Windows (server OpenSSH), trasferimento con scp.', 'Conectado: PC Windows (servidor OpenSSH), transferência por scp.'),
    'Transfert : {name}': ('Transferring: {name}', 'Übertragung: {name}', 'Transfiriendo: {name}', 'Trasferimento: {name}', 'Transferindo: {name}'),
    '{name} : délai dépassé': ('{name}: timed out', '{name}: Zeitüberschreitung', '{name}: tiempo de espera agotado', '{name}: tempo scaduto', '{name}: tempo esgotado'),
    'Transfert terminé': ('Transfer complete', 'Übertragung abgeschlossen', 'Transferencia terminada', 'Trasferimento completato', 'Transferência concluída'),
    ' ({count} erreur(s))': (' ({count} error(s))', ' ({count} Fehler)', ' ({count} error(es))', ' ({count} errore/i)', ' ({count} erro(s))'),
    "Mot de passe ou nom d'utilisateur incorrect.": ('Incorrect password or user name.', 'Falsches Kennwort oder falscher Benutzername.', 'Contraseña o nombre de usuario incorrectos.', 'Password o nome utente errati.', 'Senha ou nome de usuário incorretos.'),
    "Le PC répond mais SSH n'est pas activé (lancez « winlux-migrate --serve » dessus).": ('The PC answers but SSH is not enabled (run “winlux-migrate --serve” on it).', 'Der PC antwortet, aber SSH ist nicht aktiviert (führen Sie dort „winlux-migrate --serve“ aus).', 'El PC responde pero SSH no está activado (ejecute «winlux-migrate --serve» en él).', 'Il PC risponde ma SSH non è attivo (esegui «winlux-migrate --serve» su di esso).', 'O PC responde, mas o SSH não está ativado (execute “winlux-migrate --serve” nele).'),
    'PC introuvable : vérifiez son nom ou utilisez son adresse IP.': ('PC not found: check its name or use its IP address.', 'PC nicht gefunden: Prüfen Sie den Namen oder verwenden Sie die IP-Adresse.', 'PC no encontrado: compruebe su nombre o use su dirección IP.', "PC non trovato: controlla il nome o usa l'indirizzo IP.", 'PC não encontrado: verifique o nome ou use o endereço IP.'),
    "Le PC ne répond pas : vérifiez qu'il est allumé et sur le même réseau.": ('The PC does not answer: check that it is on and on the same network.', 'Der PC antwortet nicht: Prüfen Sie, ob er eingeschaltet und im selben Netzwerk ist.', 'El PC no responde: compruebe que está encendido y en la misma red.', 'Il PC non risponde: verifica che sia acceso e sulla stessa rete.', 'O PC não responde: verifique se está ligado e na mesma rede.'),
    'La clé SSH du PC a changé pendant la session : transfert refusé par sécurité.': ("The PC's SSH key changed during the session: transfer refused for security.", 'Der SSH-Schlüssel des PCs hat sich während der Sitzung geändert: Übertragung aus Sicherheitsgründen abgelehnt.', 'La clave SSH del PC cambió durante la sesión: transferencia rechazada por seguridad.', 'La chiave SSH del PC è cambiata durante la sessione: trasferimento rifiutato per sicurezza.', 'A chave SSH do PC mudou durante a sessão: transferência recusada por segurança.'),
    'Connexion SSH impossible.': ('Unable to connect over SSH.', 'SSH-Verbindung nicht möglich.', 'No se puede conectar por SSH.', 'Impossibile connettersi via SSH.', 'Não foi possível conectar por SSH.'),
    "SSH n'est pas actif : installez openssh-server puis relancez avec sudo.": ('SSH is not running: install openssh-server, then run again with sudo.', 'SSH ist nicht aktiv: Installieren Sie openssh-server und starten Sie erneut mit sudo.', 'SSH no está activo: instale openssh-server y vuelva a ejecutarlo con sudo.', 'SSH non è attivo: installa openssh-server e riavvia con sudo.', 'SSH não está ativo: instale o openssh-server e execute novamente com sudo.'),
    'Ce PC est visible sous « {hostname} » ({ip}). Utilisateur à saisir : {user}': ('This PC is visible as “{hostname}” ({ip}). User to enter: {user}', 'Dieser PC ist als „{hostname}“ ({ip}) sichtbar. Einzugebender Benutzer: {user}', 'Este PC es visible como «{hostname}» ({ip}). Usuario que debe introducir: {user}', 'Questo PC è visibile come «{hostname}» ({ip}). Utente da inserire: {user}', 'Este PC está visível como “{hostname}” ({ip}). Usuário a digitar: {user}'),
    'Laissez cette fenêtre ouverte pendant le transfert. Ctrl+C pour arrêter.': ('Keep this window open during the transfer. Ctrl+C to stop.', 'Lassen Sie dieses Fenster während der Übertragung geöffnet. Strg+C zum Beenden.', 'Deje esta ventana abierta durante la transferencia. Ctrl+C para detener.', 'Lascia aperta questa finestra durante il trasferimento. Ctrl+C per interrompere.', 'Deixe esta janela aberta durante a transferência. Ctrl+C para parar.'),
    'Serveur SSH arrêté.': ('SSH server stopped.', 'SSH-Server beendet.', 'Servidor SSH detenido.', 'Server SSH arrestato.', 'Servidor SSH parado.'),
    'Ne rien transférer (vous pourrez le faire plus tard avec « Transfert de PC »)': ("Don't transfer anything (you can do it later with “PC transfer”)", 'Nichts übertragen (später mit „PC-Übertragung“ möglich)', 'No transferir nada (podrá hacerlo más tarde con «Transferencia de PC»)', 'Non trasferire nulla (potrai farlo più tardi con «Trasferimento PC»)', 'Não transferir nada (você pode fazer isso depois com “Transferência de PC”)'),
    'Transférer depuis un autre PC du réseau': ('Transfer from another PC on the network', 'Von einem anderen PC im Netzwerk übertragen', 'Transferir desde otro PC de la red', 'Trasferisci da un altro PC della rete', 'Transferir de outro PC da rede'),
    'Rechercher les PC du réseau': ('Search for PCs on the network', 'PCs im Netzwerk suchen', 'Buscar PC en la red', 'Cerca PC nella rete', 'Procurar PCs na rede'),
    'Nom ou adresse IP :': ('Name or IP address:', 'Name oder IP-Adresse:', 'Nombre o dirección IP:', 'Nome o indirizzo IP:', 'Nome ou endereço IP:'),
    "Utilisateur sur l'autre PC :": ('User on the other PC:', 'Benutzer auf dem anderen PC:', 'Usuario en el otro PC:', "Utente sull'altro PC:", 'Usuário no outro PC:'),
    'Mot de passe :': ('Password:', 'Kennwort:', 'Contraseña:', 'Password:', 'Senha:'),
    'Paramètres (thème, navigateur, git, terminal)': ('Settings (theme, browser, git, terminal)', 'Einstellungen (Design, Browser, git, Terminal)', 'Configuración (tema, navegador, git, terminal)', 'Impostazioni (tema, browser, git, terminale)', 'Configurações (tema, navegador, git, terminal)'),
    'Tester la connexion': ('Test the connection', 'Verbindung testen', 'Probar la conexión', 'Prova la connessione', 'Testar a conexão'),
    "Sur l'autre PC Winlux ou Linux : lancez « sudo winlux-migrate --serve ».\nSur Windows : Paramètres › Système › Fonctionnalités facultatives › Serveur OpenSSH, puis saisissez son adresse IP. Les paramètres ne sont repris que depuis Linux/Winlux.": ('On the other Winlux or Linux PC: run “sudo winlux-migrate --serve”.\nOn Windows: Settings › System › Optional features › OpenSSH Server, then enter its IP address. Settings are only copied from Linux/Winlux.', 'Auf dem anderen Winlux- oder Linux-PC: Führen Sie „sudo winlux-migrate --serve“ aus.\nUnter Windows: Einstellungen › System › Optionale Features › OpenSSH-Server, dann seine IP-Adresse eingeben. Einstellungen werden nur von Linux/Winlux übernommen.', 'En el otro PC Winlux o Linux: ejecute «sudo winlux-migrate --serve».\nEn Windows: Configuración › Sistema › Características opcionales › Servidor OpenSSH; después introduzca su dirección IP. La configuración solo se copia desde Linux/Winlux.', "Sull'altro PC Winlux o Linux: esegui «sudo winlux-migrate --serve».\nSu Windows: Impostazioni › Sistema › Funzionalità facoltative › Server OpenSSH, poi inserisci il suo indirizzo IP. Le impostazioni vengono copiate solo da Linux/Winlux.", 'No outro PC Winlux ou Linux: execute “sudo winlux-migrate --serve”.\nNo Windows: Configurações › Sistema › Recursos opcionais › Servidor OpenSSH e digite o endereço IP. As configurações só são copiadas do Linux/Winlux.'),
    'Recherche en cours…': ('Searching…', 'Suche läuft…', 'Buscando…', 'Ricerca in corso…', 'Procurando…'),
    'Réseau indisponible : {error}': ('Network unavailable: {error}', 'Netzwerk nicht verfügbar: {error}', 'Red no disponible: {error}', 'Rete non disponibile: {error}', 'Rede indisponível: {error}'),
    '{count} PC trouvé(s). Sélectionnez-en un ou saisissez une adresse.': ('{count} PC(s) found. Select one or enter an address.', '{count} PC(s) gefunden. Wählen Sie einen aus oder geben Sie eine Adresse ein.', '{count} PC encontrado(s). Seleccione uno o introduzca una dirección.', '{count} PC trovato/i. Selezionane uno o inserisci un indirizzo.', '{count} PC(s) encontrado(s). Selecione um ou digite um endereço.'),
    "Aucun PC annoncé. Saisissez l'adresse IP de l'autre PC.": ("No PC announced. Enter the other PC's IP address.", 'Kein PC gemeldet. Geben Sie die IP-Adresse des anderen PCs ein.', 'Ningún PC anunciado. Introduzca la dirección IP del otro PC.', "Nessun PC annunciato. Inserisci l'indirizzo IP dell'altro PC.", 'Nenhum PC anunciado. Digite o endereço IP do outro PC.'),
    'Connexion…': ('Connecting…', 'Verbindung wird hergestellt…', 'Conectando…', 'Connessione…', 'Conectando…'),
    'Aucune': ('None', 'Keine', 'Ninguna', 'Nessuna', 'Nenhuma'),
    'paramètres': ('settings', 'Einstellungen', 'configuración', 'impostazioni', 'configurações'),
    'depuis {host} : {items}': ('from {host}: {items}', 'von {host}: {items}', 'desde {host}: {items}', 'da {host}: {items}', 'de {host}: {items}'),
    'rien de coché': ('nothing selected', 'nichts ausgewählt', 'nada marcado', 'niente selezionato', 'nada marcado'),
    'Transfert de PC': ('PC transfer', 'PC-Übertragung', 'Transferencia de PC', 'Trasferimento PC', 'Transferência de PC'),
    'Transférer vos fichiers depuis un autre PC': ('Transfer your files from another PC', 'Ihre Dateien von einem anderen PC übertragen', 'Transferir sus archivos desde otro PC', 'Trasferisci i tuoi file da un altro PC', 'Transferir seus arquivos de outro PC'),
    'Démarrer le transfert': ('Start the transfer', 'Übertragung starten', 'Iniciar la transferencia', 'Avvia il trasferimento', 'Iniciar a transferência'),
    'Terminé': ('Done', 'Fertig', 'Terminado', 'Completato', 'Concluído'),
}


def _lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="):
                        code = line[5:].strip()
        except OSError:
            pass
    return (code or "fr")[:2].lower()


def _t(text):
    """Texte dans la langue de l'installateur ou du système (le français sert de clé)."""
    row = _TR.get(text)
    col = _COL.get(_lang())
    return row[col] if row and col is not None else text

MDNS_ADDR = "224.0.0.251"
MDNS_PORT = 5353
SERVICE = "_winlux-migrate._tcp.local"
BROWSE_TYPES = [SERVICE, "_ssh._tcp.local", "_sftp-ssh._tcp.local"]
T_A, T_PTR, T_TXT, T_SRV = 1, 12, 16, 33

# Dossiers personnels : clé, libellé, nom Windows (sous %USERPROFILE%), clé XDG, dossier Winlux.
FOLDERS = [
    ("desktop", "Bureau", "Desktop", "DESKTOP", "Bureau"),
    ("documents", "Documents", "Documents", "DOCUMENTS", "Documents"),
    ("downloads", "Téléchargements", "Downloads", "DOWNLOAD", "Téléchargements"),
    ("pictures", "Images", "Pictures", "PICTURES", "Images"),
    ("music", "Musique", "Music", "MUSIC", "Musique"),
    ("videos", "Vidéos", "Videos", "VIDEOS", "Vidéos"),
]
# Paramètres repris d'un Linux/Winlux. Pas de clés SSH ni de trousseaux.
SETTINGS = [".config/gtk-3.0", ".config/gtk-4.0", ".config/winlux", ".gitconfig", ".bashrc",
            ".bash_aliases", ".vimrc", ".mozilla", ".thunderbird", ".local/share/fonts", ".fonts"]

HOST_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._:%-]*$")
USER_RE = re.compile(r"^[^\s@:/\\'\"`$;|&<>-][^@:/\\'\"`$;|&<>]*$")


# ── DNS ───────────────────────────────────────────────────────────

def encode_name(name):
    out = b""
    for label in name.rstrip(".").split("."):
        raw = label.encode("utf-8")[:63]
        out += bytes([len(raw)]) + raw
    return out + b"\0"


def decode_name(data, offset):
    labels, jumped, end, hops = [], False, offset, 0
    while True:
        if offset >= len(data) or hops > 64:
            raise ValueError("nom DNS invalide")
        length = data[offset]
        if length & 0xC0 == 0xC0:
            pointer = ((length & 0x3F) << 8) | data[offset + 1]
            if not jumped:
                end = offset + 2
            offset, jumped, hops = pointer, True, hops + 1
            continue
        if length == 0:
            if not jumped:
                end = offset + 1
            return ".".join(labels), end
        labels.append(data[offset + 1:offset + 1 + length].decode("utf-8", "replace"))
        offset += 1 + length


def build_query(names, qid=0):
    packet = struct.pack("!HHHHHH", qid, 0, len(names), 0, 0, 0)
    for name in names:
        packet += encode_name(name) + struct.pack("!HH", T_PTR, 1)
    return packet


def record(name, rtype, rdata, ttl=120, flush=False):
    rclass = 1 | (0x8000 if flush else 0)
    return encode_name(name) + struct.pack("!HHIH", rtype, rclass, ttl, len(rdata)) + rdata


def txt_rdata(values):
    out = b""
    for key, value in values.items():
        raw = f"{key}={value}".encode("utf-8")[:255]
        out += bytes([len(raw)]) + raw
    return out or b"\0"


def build_response(qid, questions, instance, hostname, ip, port, txt, unicast):
    """Réponse DNS-SD complète (PTR + SRV + TXT + A) pour le service de migration."""
    full = f"{instance}.{SERVICE}"
    host = f"{hostname}.local"
    answers = [record(SERVICE, T_PTR, encode_name(full))]
    additional = [
        record(full, T_SRV, struct.pack("!HHH", 0, 0, port) + encode_name(host), flush=not unicast),
        record(full, T_TXT, txt_rdata(txt), flush=not unicast),
        record(host, T_A, socket.inet_aton(ip), flush=not unicast),
    ]
    qd = b""
    if unicast:
        qd = b"".join(encode_name(q) + struct.pack("!HH", T_PTR, 1) for q in questions)
    header = struct.pack("!HHHHHH", qid if unicast else 0, 0x8400, len(questions) if unicast else 0,
                         len(answers), 0, len(additional))
    return header + qd + b"".join(answers) + b"".join(additional)


def parse_packet(data):
    qid, flags, qd, an, ns, ar = struct.unpack("!HHHHHH", data[:12])
    offset = 12
    questions = []
    for _ in range(qd):
        name, offset = decode_name(data, offset)
        qtype, _qclass = struct.unpack("!HH", data[offset:offset + 4])
        offset += 4
        questions.append((name, qtype))
    records = []
    for _ in range(an + ns + ar):
        name, offset = decode_name(data, offset)
        rtype, _rclass, _ttl, length = struct.unpack("!HHIH", data[offset:offset + 10])
        offset += 10
        rdata_offset = offset
        offset += length
        if rtype == T_PTR:
            value = decode_name(data, rdata_offset)[0]
        elif rtype == T_SRV:
            _prio, _weight, port = struct.unpack("!HHH", data[rdata_offset:rdata_offset + 6])
            value = (port, decode_name(data, rdata_offset + 6)[0])
        elif rtype == T_A and length == 4:
            value = socket.inet_ntoa(data[rdata_offset:offset])
        elif rtype == T_TXT:
            value, i = {}, rdata_offset
            while i < offset:
                n = data[i]
                entry = data[i + 1:i + 1 + n].decode("utf-8", "replace")
                if "=" in entry:
                    k, v = entry.split("=", 1)
                    value[k] = v
                i += 1 + n
        else:
            value = None
        records.append((name, rtype, value))
    return {"id": qid, "response": bool(flags & 0x8000), "questions": questions, "records": records}


def hosts_from_records(records, sender_ip):
    ptrs = [(name, value) for name, rtype, value in records if rtype == T_PTR]
    srv = {name: value for name, rtype, value in records if rtype == T_SRV}
    txt = {name: value for name, rtype, value in records if rtype == T_TXT}
    addr = {name: value for name, rtype, value in records if rtype == T_A}
    found = []
    for service, instance in ptrs:
        if service not in BROWSE_TYPES:
            continue
        port, target = srv.get(instance, (22, ""))
        label = instance[:-len(service) - 1] if instance.endswith("." + service) else instance
        meta = txt.get(instance, {})
        found.append({
            "name": label,
            "host": target.rstrip(".") if target else sender_ip,
            "ip": addr.get(target, sender_ip),
            "port": port if service == SERVICE else 22 if port == 0 else port,
            "kind": "winlux" if service == SERVICE else "ssh",
            "os": meta.get("os", "linux" if service != SERVICE else "winlux"),
            "user": meta.get("user", ""),
        })
    return found


def scan(timeout=3.0):
    """Interroge le réseau ; les réponses reviennent en unicast vers notre port éphémère."""
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_TTL, 255)
    sock.bind(("0.0.0.0", 0))
    found = {}
    try:
        query = build_query(BROWSE_TYPES, qid=os.getpid() & 0xFFFF)
        end = time.monotonic() + timeout
        next_send = 0.0
        while True:
            now = time.monotonic()
            if now >= end:
                break
            if now >= next_send:
                try:
                    sock.sendto(query, (MDNS_ADDR, MDNS_PORT))
                except OSError:
                    break
                next_send = now + 1.0
            ready, _, _ = select.select([sock], [], [], min(0.25, end - now))
            if not ready:
                continue
            data, (ip, _port) = sock.recvfrom(9000)
            try:
                packet = parse_packet(data)
            except (ValueError, struct.error, IndexError):
                continue
            if not packet["response"]:
                continue
            for h in hosts_from_records(packet["records"], ip):
                key = h["ip"]
                # Un PC Winlux annonce aussi souvent _ssh : on garde l'annonce Winlux.
                if key not in found or h["kind"] == "winlux":
                    found[key] = h
    finally:
        sock.close()
    return sorted(found.values(), key=lambda h: (h["kind"] != "winlux", h["name"].lower()))


def local_ip_towards(addr):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        s.connect((addr, 9))
        return s.getsockname()[0]
    except OSError:
        return "127.0.0.1"
    finally:
        s.close()


def serve_mdns(instance, hostname, port, txt, stop):
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    if hasattr(socket, "SO_REUSEPORT"):
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
    sock.bind(("", MDNS_PORT))
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP,
                    socket.inet_aton(MDNS_ADDR) + socket.inet_aton("0.0.0.0"))
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_TTL, 255)
    wanted = {SERVICE.lower(), "_services._dns-sd._udp.local"}
    try:
        while not stop.is_set():
            ready, _, _ = select.select([sock], [], [], 0.5)
            if not ready:
                continue
            data, (src, sport) = sock.recvfrom(9000)
            try:
                packet = parse_packet(data)
            except (ValueError, struct.error, IndexError):
                continue
            if packet["response"]:
                continue
            asked = [q for q, t in packet["questions"] if q.lower() in wanted and t in (T_PTR, 255)]
            if not asked:
                continue
            unicast = sport != MDNS_PORT
            reply = build_response(packet["id"], [SERVICE] if unicast else [], instance, hostname,
                                   local_ip_towards(src), port, txt, unicast)
            sock.sendto(reply, (src, sport) if unicast else (MDNS_ADDR, MDNS_PORT))
    finally:
        sock.close()


# ── SSH / transfert ───────────────────────────────────────────────

def validate(host, user, port=22):
    host, user = host.strip(), user.strip()
    if not HOST_RE.match(host):
        raise ValueError(_t("Adresse invalide (nom de machine ou adresse IP attendue)."))
    if not USER_RE.match(user):
        raise ValueError(_t("Nom d'utilisateur invalide."))
    if not 1 <= int(port) <= 65535:
        raise ValueError(_t("Port invalide."))
    return host, user, int(port)


def rsync_host(host):
    try:
        if ipaddress.ip_address(host.split("%")[0]).version == 6:
            return f"[{host}]"
    except ValueError:
        pass
    return host


class Session:
    """Connexion SSH par mot de passe sans sshpass : SSH_ASKPASS lit une variable d'environnement.

    Les empreintes d'hôte sont gardées dans un known_hosts propre à la session :
    la clé acceptée au test de connexion doit être la même pendant le transfert.
    """

    def __init__(self, host, user, password, port=22):
        self.host, self.user, self.port = validate(host, user, port)
        self.password = password
        self.dir = tempfile.mkdtemp(prefix="winlux-migrate-")
        os.chmod(self.dir, 0o700)
        self.askpass = os.path.join(self.dir, "askpass")
        with open(self.askpass, "w") as f:
            f.write("#!/bin/sh\nprintf '%s\\n' \"$WINLUX_MIGRATE_PASS\"\n")
        os.chmod(self.askpass, 0o700)
        self.known_hosts = os.path.join(self.dir, "known_hosts")
        self.remote_os = None
        self.has_rsync = False
        self.remote_home = ""
        self.remote_dirs = {}

    def close(self):
        shutil.rmtree(self.dir, ignore_errors=True)

    def env(self):
        env = dict(os.environ)
        env.update({"SSH_ASKPASS": self.askpass, "SSH_ASKPASS_REQUIRE": "force",
                    "WINLUX_MIGRATE_PASS": self.password, "DISPLAY": env.get("DISPLAY", ":0")})
        return env

    def ssh_options(self):
        return ["-o", "StrictHostKeyChecking=accept-new", "-o", f"UserKnownHostsFile={self.known_hosts}",
                "-o", "ConnectTimeout=8", "-o", "PubkeyAuthentication=no",
                "-o", "PreferredAuthentications=password,keyboard-interactive",
                "-o", "NumberOfPasswordPrompts=1", "-o", "ServerAliveInterval=15"]

    def ssh(self, command, timeout=30):
        argv = ["ssh", *self.ssh_options(), "-p", str(self.port), "-l", self.user, "--", self.host, command]
        return subprocess.run(argv, env=self.env(), stdin=subprocess.DEVNULL, capture_output=True,
                              text=True, timeout=timeout)

    def probe(self):
        """Teste la connexion et détecte le système distant. Retourne un message lisible."""
        result = self.ssh("echo WINLUX_PROBE && uname -s")
        if result.returncode == 255:
            raise ConnectionError(explain_ssh_error(result.stderr))
        out = result.stdout
        if "Linux" in out or "Darwin" in out or "BSD" in out:
            self.remote_os = "linux"
            script = ("printf 'HOME=%s\\n' \"$HOME\"; command -v rsync >/dev/null && echo RSYNC=1; "
                      "for d in DESKTOP DOCUMENTS DOWNLOAD PICTURES MUSIC VIDEOS; do "
                      "printf '%s=%s\\n' \"$d\" \"$(xdg-user-dir $d 2>/dev/null)\"; done")
            details = self.ssh(script).stdout
            values = dict(line.split("=", 1) for line in details.splitlines() if "=" in line)
            self.remote_home = values.get("HOME", "")
            self.has_rsync = values.get("RSYNC") == "1"
            for key, _label, english, xdg, _local in FOLDERS:
                path = values.get(xdg, "")
                if not path or path.rstrip("/") == self.remote_home.rstrip("/"):
                    path = f"{self.remote_home}/{english}" if self.remote_home else english
                self.remote_dirs[key] = path
            tool = "rsync" if self.has_rsync else _t("scp (rsync absent sur l'autre PC)")
            return _t("Connecté : PC Linux/Winlux, transfert par {tool}.").format(tool=tool)
        self.remote_os = "windows"
        self.remote_dirs = {key: english for key, _label, english, _xdg, _local in FOLDERS}
        return _t("Connecté : PC Windows (serveur OpenSSH), transfert par scp.")

    def pull(self, home, folders, settings, progress=None, uid=None, gid=None):
        """Copie les éléments choisis vers `home`. Retourne la liste des erreurs (non bloquantes)."""
        if self.remote_os is None:
            self.probe()
        errors = []
        items = []
        local_names = {key: local for key, _l, _e, _x, local in FOLDERS}
        for key in folders:
            items.append((self.remote_dirs[key], os.path.join(home, local_names[key]), True))
        if settings and self.remote_os == "linux":
            for rel in SETTINGS:
                items.append((f"{self.remote_home}/{rel}", os.path.join(home, rel), False))
        for index, (remote, local, is_folder) in enumerate(items):
            if progress:
                progress(index / max(1, len(items)), _t("Transfert : {name}").format(name=os.path.basename(local)))
            os.makedirs(os.path.dirname(local), exist_ok=True)
            try:
                rc, err = self.copy(remote, local, is_folder)
                # 23/24 : fichiers absents ou disparus pendant la copie (rsync), non bloquant.
                if rc not in (0, 23, 24) and is_folder:
                    errors.append(f"{os.path.basename(local)} : {err.strip()[:200] or f'code {rc}'}")
            except subprocess.TimeoutExpired:
                errors.append(_t("{name} : délai dépassé").format(name=os.path.basename(local)))
        if uid is not None:
            subprocess.run(["chown", "-R", f"{uid}:{gid}", home], timeout=600)
        if progress:
            progress(1.0, _t("Transfert terminé") + (_t(" ({count} erreur(s))").format(count=len(errors)) if errors else ""))
        return errors

    def copy(self, remote, local, is_folder):
        if self.remote_os == "linux" and self.has_rsync and shutil.which("rsync"):
            ssh_cmd = " ".join(["ssh", *self.ssh_options(), "-p", str(self.port)])
            source = f"{self.user}@{rsync_host(self.host)}:{remote}" + ("/" if is_folder else "")
            # Un paramètre (fichier ou dossier) est copié dans son dossier parent.
            target = local + "/" if is_folder else os.path.dirname(local) + "/"
            if is_folder:
                os.makedirs(local, exist_ok=True)
            argv = ["rsync", "-rlt", "--protect-args", "--partial", "--ignore-missing-args",
                    "-e", ssh_cmd, source, target]
        else:
            if is_folder and os.path.isdir(local) and not os.listdir(local):
                os.rmdir(local)
            if is_folder and os.path.isdir(local):
                # Dossier déjà rempli : scp crée un sous-dossier, on le garde à part.
                local = os.path.join(local, os.path.basename(remote.rstrip("/")) + " (ancien PC)")
            target = local if is_folder else os.path.dirname(local) + "/"
            argv = ["scp", "-r", "-p", *self.ssh_options(), "-P", str(self.port),
                    "-o", f"User={self.user}", "--", f"{rsync_host(self.host)}:{remote}", target]
        result = subprocess.run(argv, env=self.env(), stdin=subprocess.DEVNULL, capture_output=True,
                                text=True, timeout=6 * 3600)
        if not is_folder and result.returncode != 0:
            return 0, ""
        return result.returncode, result.stderr


def explain_ssh_error(stderr):
    text = stderr.lower()
    if "permission denied" in text:
        return _t("Mot de passe ou nom d'utilisateur incorrect.")
    if "connection refused" in text:
        return _t("Le PC répond mais SSH n'est pas activé (lancez « winlux-migrate --serve » dessus).")
    if "could not resolve" in text or "name or service not known" in text:
        return _t("PC introuvable : vérifiez son nom ou utilisez son adresse IP.")
    if "timed out" in text or "no route" in text or "unreachable" in text:
        return _t("Le PC ne répond pas : vérifiez qu'il est allumé et sur le même réseau.")
    if "host key verification failed" in text or "remote host identification has changed" in text:
        return _t("La clé SSH du PC a changé pendant la session : transfert refusé par sécurité.")
    return stderr.strip().splitlines()[-1] if stderr.strip() else _t("Connexion SSH impossible.")


# ── Côté source : --serve ─────────────────────────────────────────

def sshd_listening(port=22):
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=1):
            return True
    except OSError:
        return False


def serve(port=22):
    hostname = socket.gethostname().split(".")[0] or "winlux"
    user = os.environ.get("SUDO_USER") or os.environ.get("USER") or ""
    started = None
    if not sshd_listening(port):
        sshd = shutil.which("sshd") or "/usr/sbin/sshd"
        if os.geteuid() != 0 or not os.path.exists(sshd):
            print(_t("SSH n'est pas actif : installez openssh-server puis relancez avec sudo."), file=sys.stderr)
            return 1
        subprocess.run(["ssh-keygen", "-A"], check=False)
        os.makedirs("/run/sshd", exist_ok=True)
        started = subprocess.Popen([sshd, "-D", "-p", str(port), "-o", "PasswordAuthentication=yes"])
        time.sleep(1)
    stop = threading.Event()
    txt = {"os": "winlux" if os.path.exists("/usr/share/winlux/version") else "linux", "user": user, "v": "1"}
    worker = threading.Thread(target=serve_mdns, args=(hostname, hostname, port, txt, stop), daemon=True)
    worker.start()
    ip = local_ip_towards(MDNS_ADDR)
    print(_t("Ce PC est visible sous « {hostname} » ({ip}). Utilisateur à saisir : {user}").format(hostname=hostname, ip=ip, user=user or "?"))
    print(_t("Laissez cette fenêtre ouverte pendant le transfert. Ctrl+C pour arrêter."))
    try:
        while worker.is_alive():
            worker.join(0.5)
    except KeyboardInterrupt:
        pass
    finally:
        stop.set()
        if started:
            started.terminate()
            print(_t("Serveur SSH arrêté."))
    return 0


# ── Interface GTK (partagée avec l'installateur) ──────────────────

def build_panel(Gtk, GLib, on_change=None):
    """Panneau de migration réutilisable : retourne (widget, contrôleur)."""
    return MigrationPanel(Gtk, GLib, on_change)


class MigrationPanel:
    def __init__(self, Gtk, GLib, on_change=None):
        self.Gtk, self.GLib, self.on_change = Gtk, GLib, on_change
        self.enabled = False
        self.session = None
        self.connected = False
        self.hosts = []
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.widget = box
        skip = Gtk.RadioButton.new_with_label(None, _t("Ne rien transférer (vous pourrez le faire plus tard avec « Transfert de PC »)"))
        use = Gtk.RadioButton.new_with_label_from_widget(skip, _t("Transférer depuis un autre PC du réseau"))
        for rb in (skip, use):
            card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
            card.get_style_context().add_class("card")
            card.pack_start(rb, False, False, 0)
            box.pack_start(card, False, False, 0)
        use.connect("toggled", self._on_enable)
        self.form = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        box.pack_start(self.form, True, True, 0)
        scan_row = Gtk.Box(spacing=12)
        self.scan_btn = Gtk.Button(label=_t("Rechercher les PC du réseau"))
        self.scan_btn.get_style_context().add_class("btn-secondary")
        self.scan_btn.connect("clicked", self._on_scan)
        scan_row.pack_start(self.scan_btn, False, False, 0)
        self.scan_status = Gtk.Label(label="", xalign=0)
        self.scan_status.get_style_context().add_class("card-detail")
        scan_row.pack_start(self.scan_status, True, True, 0)
        self.form.pack_start(scan_row, False, False, 0)
        self.host_list = Gtk.ListBox()
        self.host_list.get_style_context().add_class("card")
        self.host_list.connect("row-selected", self._on_host_selected)
        self.host_list.set_no_show_all(True)
        self.form.pack_start(self.host_list, False, False, 0)
        grid = Gtk.Grid(column_spacing=12, row_spacing=8)
        self.entries = {}
        for row, (key, label, secret) in enumerate([("host", _t("Nom ou adresse IP :"), False),
                                                    ("user", _t("Utilisateur sur l'autre PC :"), False),
                                                    ("password", _t("Mot de passe :"), True)]):
            lbl = Gtk.Label(label=label, xalign=1)
            lbl.get_style_context().add_class("input-label")
            grid.attach(lbl, 0, row, 1, 1)
            entry = Gtk.Entry()
            entry.get_style_context().add_class("input-field")
            entry.set_hexpand(True)
            if secret:
                entry.set_visibility(False)
                entry.set_input_purpose(Gtk.InputPurpose.PASSWORD)
            entry.connect("changed", self._on_field)
            grid.attach(entry, 1, row, 1, 1)
            self.entries[key] = entry
        self.form.pack_start(grid, False, False, 0)
        checks = Gtk.FlowBox()
        checks.set_selection_mode(Gtk.SelectionMode.NONE)
        checks.set_max_children_per_line(4)
        self.checks = {}
        for key, label, *_ in FOLDERS:
            cb = Gtk.CheckButton(label=_t(label))
            cb.set_active(key != "downloads")
            checks.add(cb)
            self.checks[key] = cb
        self.settings_check = Gtk.CheckButton(label=_t("Paramètres (thème, navigateur, git, terminal)"))
        self.settings_check.set_active(True)
        checks.add(self.settings_check)
        self.form.pack_start(checks, False, False, 0)
        test_row = Gtk.Box(spacing=12)
        self.test_btn = Gtk.Button(label=_t("Tester la connexion"))
        self.test_btn.get_style_context().add_class("btn-primary")
        self.test_btn.connect("clicked", self._on_test)
        test_row.pack_start(self.test_btn, False, False, 0)
        self.test_status = Gtk.Label(label="", xalign=0)
        self.test_status.set_line_wrap(True)
        self.test_status.get_style_context().add_class("card-detail")
        test_row.pack_start(self.test_status, True, True, 0)
        self.form.pack_start(test_row, False, False, 0)
        hint = Gtk.Label(xalign=0, label=(
            _t("Sur l'autre PC Winlux ou Linux : lancez « sudo winlux-migrate --serve ».\n"
               "Sur Windows : Paramètres › Système › Fonctionnalités facultatives › Serveur OpenSSH, "
               "puis saisissez son adresse IP. Les paramètres ne sont repris que depuis Linux/Winlux.")))
        hint.set_line_wrap(True)
        hint.get_style_context().add_class("card-detail")
        self.form.pack_start(hint, False, False, 0)
        self.form.set_sensitive(False)

    def _changed(self):
        if self.on_change:
            self.on_change()

    def ready(self):
        return (not self.enabled) or self.connected

    def _on_enable(self, btn):
        self.enabled = btn.get_active()
        self.form.set_sensitive(self.enabled)
        if self.enabled and not self.hosts:
            self._on_scan(None)
        self._changed()

    def _on_field(self, _entry):
        self.connected = False
        self.test_status.set_text("")
        self._changed()

    def _on_scan(self, _btn):
        self.scan_btn.set_sensitive(False)
        self.scan_status.set_text(_t("Recherche en cours…"))

        def work():
            try:
                hosts = scan()
            except OSError as e:
                hosts, error = [], str(e)
            else:
                error = ""
            self.GLib.idle_add(self._scan_done, hosts, error)
        threading.Thread(target=work, daemon=True).start()

    def _scan_done(self, hosts, error):
        self.hosts = hosts
        for child in self.host_list.get_children():
            child.destroy()
        for h in hosts:
            kind = {"winlux": "Winlux", "linux": "Linux"}.get(h["os"], h["os"].capitalize())
            label = self.Gtk.Label(label=f"{h['name']}  —  {h['ip']}  ·  {kind}", xalign=0)
            label.get_style_context().add_class("card-title")
            self.host_list.add(label)
            label.show()
        self.host_list.set_visible(bool(hosts))
        self.scan_btn.set_sensitive(True)
        if error:
            self.scan_status.set_text(_t("Réseau indisponible : {error}").format(error=error))
        elif hosts:
            self.scan_status.set_text(_t("{count} PC trouvé(s). Sélectionnez-en un ou saisissez une adresse.").format(count=len(hosts)))
        else:
            self.scan_status.set_text(_t("Aucun PC annoncé. Saisissez l'adresse IP de l'autre PC."))
        return False

    def _on_host_selected(self, _box, row):
        if row is None:
            return
        h = self.hosts[row.get_index()]
        self.entries["host"].set_text(h["ip"])
        if h["user"] and not self.entries["user"].get_text():
            self.entries["user"].set_text(h["user"])
        self.selected = h

    def _on_test(self, _btn):
        host = self.entries["host"].get_text()
        user = self.entries["user"].get_text()
        password = self.entries["password"].get_text()
        selected = getattr(self, "selected", None)
        port = selected["port"] if selected and selected["ip"] == host.strip() else 22
        self.test_btn.set_sensitive(False)
        self.test_status.set_text(_t("Connexion…"))

        def work():
            if self.session:
                self.session.close()
            try:
                self.session = Session(host, user, password, port)
                message, ok = self.session.probe(), True
            except (ValueError, ConnectionError, OSError, subprocess.SubprocessError) as e:
                message, ok = str(e), False
            self.GLib.idle_add(self._test_done, message, ok)
        threading.Thread(target=work, daemon=True).start()

    def _test_done(self, message, ok):
        self.connected = ok
        self.test_status.set_text(("✓ " if ok else "✗ ") + message)
        self.test_btn.set_sensitive(True)
        self._changed()
        return False

    def selection(self):
        folders = [key for key, cb in self.checks.items() if cb.get_active()]
        return folders, self.settings_check.get_active()

    def describe(self):
        if not self.enabled or not self.session:
            return _t("Aucune")
        folders, settings = self.selection()
        names = [_t(label) for key, label, *_ in FOLDERS if key in folders]
        if settings and self.session.remote_os == "linux":
            names.append(_t("paramètres"))
        return _t("depuis {host} : {items}").format(host=self.session.host, items=", ".join(names) or _t("rien de coché"))


def run_gui():
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk, GLib
    win = Gtk.Window(title=_t("Transfert de PC"))
    win.set_default_size(720, 560)
    win.connect("destroy", Gtk.main_quit)
    outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
    outer.set_border_width(24)
    win.add(outer)
    title = Gtk.Label(label=_t("Transférer vos fichiers depuis un autre PC"), xalign=0)
    title.get_style_context().add_class("page-title")
    outer.pack_start(title, False, False, 0)
    progress = Gtk.ProgressBar(show_text=True)
    start = Gtk.Button(label=_t("Démarrer le transfert"))
    panel = MigrationPanel(Gtk, GLib, lambda: start.set_sensitive(panel.enabled and panel.connected))
    outer.pack_start(panel.widget, True, True, 0)
    outer.pack_start(progress, False, False, 0)
    start.set_sensitive(False)
    outer.pack_start(start, False, False, 0)

    def on_start(_btn):
        start.set_sensitive(False)
        folders, settings = panel.selection()

        def report(fraction, text):
            GLib.idle_add(lambda: (progress.set_fraction(fraction), progress.set_text(text)) and False)

        def work():
            errors = panel.session.pull(os.path.expanduser("~"), folders, settings, report)
            GLib.idle_add(lambda: progress.set_text(_t("Terminé") if not errors else "; ".join(errors)) and False)
        threading.Thread(target=work, daemon=True).start()
    start.connect("clicked", on_start)
    win.show_all()
    Gtk.main()
    if panel.session:
        panel.session.close()
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--scan", action="store_true", help="lister les PC annoncés sur le réseau")
    parser.add_argument("--serve", action="store_true", help="rendre ce PC visible pour une migration")
    parser.add_argument("--port", type=int, default=22)
    parser.add_argument("--timeout", type=float, default=3.0)
    args = parser.parse_args()
    if args.scan:
        print(json.dumps(scan(args.timeout), ensure_ascii=False, indent=2))
        return 0
    if args.serve:
        return serve(args.port)
    return run_gui()


if __name__ == "__main__":
    sys.exit(main())
