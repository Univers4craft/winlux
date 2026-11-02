#!/usr/bin/python3
"""Téléphone connecté : un faux téléphone Android (protocole KDE Connect v8)
face au vrai service winlux-phone, sur la boucle locale.

Vérifie : annonce UDP et liaison TLS (Winlux client, certificat présenté),
identité renvoyée sous TLS, rien d'accepté avant l'appairage, code de
vérification, appairage depuis le téléphone puis depuis le PC, certificat
épinglé (un imposteur au même identifiant est refusé), notifications
(affichage, retrait, réponse, « Ignorer »), SMS et contacts, fichiers dans
les deux sens (nom de fichier assaini), batterie, réglages par fonction,
dissociation, règles du pare-feu (réseaux privés seulement) et page des
Paramètres masquée tant qu'aucun téléphone n'est associé.
"""
import base64
from importlib.machinery import SourceFileLoader
import importlib.util
import json
import os
from pathlib import Path
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "winlux-phone.py"
work = Path(tempfile.mkdtemp(prefix="winlux-phone-test."))
base = 40000 + os.getpid() % 2000 * 10
PORT, PHONE_UDP, PAYLOAD = base, base + 1, base + 100

env = dict(os.environ, HOME=str(work / "home"), WINLUX_PHONE_CONFIG=str(work / "config"),
           WINLUX_PHONE_DATA=str(work / "data"), WINLUX_PHONE_RUNTIME=str(work / "run"),
           WINLUX_PHONE_PORT=str(PORT), WINLUX_PHONE_REMOTE_PORT=str(PHONE_UDP),
           WINLUX_PHONE_PAYLOAD_PORT=str(PAYLOAD), WINLUX_PHONE_PEERS=f"127.0.0.1:{PHONE_UDP}",
           WINLUX_PHONE_NO_DBUS="1", WINLUX_PHONE_NO_GUI="1", WINLUX_PHONE_NO_FIREWALL="1",
           WINLUX_FIREWALL_CONF=str(work / "firewall.conf"), WINLUX_LANG="fr")
(work / "home/Téléchargements").mkdir(parents=True)
os.environ.update(env)
loader = SourceFileLoader("winlux_phone", str(SCRIPT))
spec = importlib.util.spec_from_loader("winlux_phone", loader)
phone_module = importlib.util.module_from_spec(spec)
loader.exec_module(phone_module)

failures = []


def check(condition, message):
    print(("ok   " if condition else "ÉCHEC ") + message)
    if not condition:
        failures.append(message)


def cli(*args):
    result = subprocess.run([sys.executable, "-B", str(SCRIPT), *args], env=env, capture_output=True,
                            text=True, timeout=30)
    return result.returncode, result.stdout, result.stderr


def status():
    return json.loads(cli("status")[1])


def wait(predicate, message, timeout=10):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if predicate():
                check(True, message)
                return True
        except (OSError, ValueError, KeyError, StopIteration):
            pass
        time.sleep(0.1)
    check(False, message)
    return False


def make_cert(folder, device_id):
    folder.mkdir(parents=True, exist_ok=True)
    subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "30",
                    "-subj", f"/O=KDE/OU=Kde connect/CN={device_id}",
                    "-keyout", str(folder / "key.pem"), "-out", str(folder / "cert.pem")],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return folder / "cert.pem", folder / "key.pem"


class FakePhone:
    """Comportement de l'application Android KDE Connect (côté réseau)."""

    def __init__(self, name, device_id, folder):
        self.id, self.name = device_id, name
        self.cert, self.key = make_cert(folder, device_id)
        self.der = phone_module.pem_to_der(self.cert.read_text())
        self.packets = []
        self.cond = threading.Condition()
        self.tls = None

    def identity(self):
        return {"deviceId": self.id, "deviceName": self.name, "deviceType": "phone", "protocolVersion": 8,
                "incomingCapabilities": phone_module.OUTGOING, "outgoingCapabilities": phone_module.INCOMING,
                "tcpPort": 1716}

    def discover(self):
        """Annonce UDP, attend la réponse de Winlux puis ouvre la liaison."""
        udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        udp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        udp.bind(("127.0.0.1", PHONE_UDP))
        udp.settimeout(10)
        udp.sendto(phone_module.make_packet("kdeconnect.identity", self.identity()), ("127.0.0.1", PORT))
        while True:
            data, _ = udp.recvfrom(65536)
            packet = json.loads(data)
            if packet["body"].get("deviceId") != self.id:
                break
        udp.close()
        self.winlux_identity = packet["body"]
        return self.connect(packet["body"]["tcpPort"])

    def connect(self, tcp_port, expect_ok=True):
        raw = socket.create_connection(("127.0.0.1", tcp_port), timeout=10)
        raw.sendall(phone_module.make_packet("kdeconnect.identity", self.identity()))
        # Le téléphone qui se connecte est serveur TLS et exige le certificat du PC.
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(str(self.cert), str(self.key))
        context.verify_mode = ssl.CERT_REQUIRED
        context.load_verify_locations(str(work / "config/certificate.pem"))
        tls = context.wrap_socket(raw, server_side=True)
        self.winlux_der = tls.getpeercert(binary_form=True)
        tls.sendall(phone_module.make_packet("kdeconnect.identity", self.identity()))
        self.reader = tls.makefile("rb")
        second = json.loads(self.reader.readline())
        self.tls_identity = second
        self.tls = tls
        threading.Thread(target=self.read_loop, daemon=True).start()
        return tls

    def read_loop(self):
        try:
            for line in self.reader:
                with self.cond:
                    self.packets.append(json.loads(line))
                    self.cond.notify_all()
        except (OSError, ValueError):
            pass
        with self.cond:
            self.packets.append({"type": "#closed"})
            self.cond.notify_all()

    def send(self, kind, body, payload_size=None, port=None):
        self.tls.sendall(phone_module.make_packet(kind, body, payload_size, port))

    def expect(self, kind, timeout=10, where=None):
        deadline = time.time() + timeout
        with self.cond:
            while time.time() < deadline:
                for packet in self.packets:
                    if packet["type"] == kind and (where is None or where(packet["body"])):
                        self.packets.remove(packet)
                        return packet
                self.cond.wait(0.2)
        return None


def device(device_id):
    return next(d for d in status()["devices"] if d["id"] == device_id)


daemon = subprocess.Popen([sys.executable, "-B", str(SCRIPT), "serve"], env=env,
                          stderr=open(work / "service.log", "wb"))
try:
    wait(lambda: (work / "run/control.sock").exists() and status().get("ok"), "le service démarre et répond")
    info = status()
    check(len(info["id"]) == 32 and info["name"], "identifiant et nom de ce PC publiés")
    check(oct((work / "config/private-key.pem").stat().st_mode)[-3:] == "600", "clé privée lisible du seul compte")
    code, _, _ = cli("serve", "--if-paired")
    check(code == 0, "« serve --if-paired » sans téléphone associé ne lance rien")

    phone = FakePhone("Pixel de test", "a1b2c3d4e5f60718293a4b5c6d7e8f90", work / "phone")
    phone.discover()
    check(phone.winlux_identity["protocolVersion"] == 8 and "kdeconnect.sms.messages" in phone.winlux_identity["incomingCapabilities"],
          "annonce UDP de Winlux : protocole 8 et capacités")
    check(phone.winlux_der == phone_module.pem_to_der((work / "config/certificate.pem").read_text()),
          "Winlux présente son certificat au téléphone (TLS)")
    check(phone_module.certificate_parts(phone.winlux_der)[0] == info["id"], "certificat au nom de l'identifiant du PC")
    check(phone.tls_identity["body"]["deviceId"] == info["id"], "identité renvoyée sous TLS (version 8)")
    wait(lambda: device(phone.id)["connected"] and not device(phone.id)["paired"], "téléphone visible, non associé")

    # Rien n'est accepté avant l'appairage.
    phone.send("kdeconnect.notification", {"id": "n0", "appName": "Espion", "title": "x", "text": "y"})
    check(phone.expect("kdeconnect.pair", where=lambda b: b.get("pair") is False) is not None,
          "paquet d'un téléphone non associé refusé (pair: false)")
    check(not (work / f"data/{phone.id}/notifications.json").exists(), "aucune notification enregistrée sans appairage")
    check(phone_module.paired_devices() == {}, "Paramètres : aucune page tant que rien n'est associé")

    # Appairage demandé par le téléphone, accepté sur le PC.
    stamp = int(time.time())
    phone.send("kdeconnect.pair", {"pair": True, "timestamp": stamp})
    wait(lambda: device(phone.id)["pair_request"], "demande d'appairage affichée")
    key = device(phone.id)["pair_request"]["key"]
    expected = phone_module.verification_key(phone.der, phone.winlux_der, stamp)
    check(len(key) == 8 and key == expected, f"même code de vérification des deux côtés ({key})")
    check(cli("accept", phone.id)[0] == 0, "« Accepter » sur le PC")
    check(phone.expect("kdeconnect.pair", where=lambda b: b.get("pair") is True) is not None, "le téléphone reçoit l'acceptation")
    saved = phone_module.paired_devices().get(phone.id, {})
    check(base64.b64decode(saved.get("cert", "")) == phone.der, "certificat du téléphone épinglé")
    for kind in ("kdeconnect.notification.request", "kdeconnect.sms.request_conversations",
                 "kdeconnect.contacts.request_all_uids_timestamps", "kdeconnect.battery.request"):
        check(phone.expect(kind) is not None, f"à la connexion, Winlux demande {kind}")

    # Notifications.
    note = {"id": "0|com.whatsapp|1", "appName": "WhatsApp", "title": "Alice", "text": "On mange à midi ?",
            "isClearable": True, "requestReplyId": "r-42", "time": str(int(time.time() * 1000))}
    phone.send("kdeconnect.notification", note)
    notes = work / f"data/{phone.id}/notifications.json"
    wait(lambda: json.loads(notes.read_text())[0]["text"] == "On mange à midi ?", "notification du téléphone enregistrée")
    check(json.loads(notes.read_text())[0]["time"] > 0, "heure de la notification lue (texte ou nombre)")
    phone.send("kdeconnect.notification", {"id": note["id"], "isCancel": True})
    wait(lambda: json.loads(notes.read_text()) == [], "notification retirée quand le téléphone la retire")
    phone.send("kdeconnect.notification", note)
    wait(lambda: len(json.loads(notes.read_text())) == 1, "notification de retour")
    phone_module.request({"cmd": "reply", "device": phone.id, "reply": "r-42", "text": "Oui, à midi"})
    check(phone.expect("kdeconnect.notification.reply", where=lambda b: b == {"requestReplyId": "r-42", "message": "Oui, à midi"}) is not None,
          "réponse envoyée depuis le PC")
    check(cli("dismiss", phone.id, note["id"])[0] == 0, "« Ignorer » sur le PC")
    check(phone.expect("kdeconnect.notification.request", where=lambda b: b.get("cancel") == note["id"]) is not None,
          "la notification est retirée du téléphone")
    check(json.loads(notes.read_text()) == [], "et de la liste du PC")

    # SMS et contacts.
    phone.send("kdeconnect.sms.messages", {"messages": [
        {"_id": 7, "thread_id": 3, "body": "Salut !", "addresses": [{"address": "+33 6 12 34 56 78"}],
         "date": 1000, "type": 1, "read": 1},
        {"_id": 8, "thread_id": 3, "body": "Ça va ?", "addresses": [{"address": "+33 6 12 34 56 78"}],
         "date": 2000, "type": 2, "read": 1}]})
    sms = work / f"data/{phone.id}/sms.json"
    wait(lambda: [m["body"] for m in json.loads(sms.read_text())["3"]] == ["Salut !", "Ça va ?"], "conversation SMS enregistrée dans l'ordre")
    phone.send("kdeconnect.contacts.response_uids_timestamps", {"uids": ["c1"], "c1": 1})
    check(phone.expect("kdeconnect.contacts.request_vcards_by_uid", where=lambda b: b.get("uids") == ["c1"]) is not None,
          "fiches de contacts demandées")
    phone.send("kdeconnect.contacts.response_vcards", {"uids": ["c1"],
               "c1": "BEGIN:VCARD\nVERSION:2.1\nFN:Alice Martin\nTEL;CELL:06 12 34 56 78\nEND:VCARD"})
    contacts = work / f"data/{phone.id}/contacts.json"
    wait(lambda: phone_module.contact_name(json.loads(contacts.read_text()), "+33612345678") == "Alice Martin",
         "numéro international reconnu comme « Alice Martin »")
    check(cli("sms", phone.id, "+33612345678", "Rendez-vous", "à", "8 h")[0] == 0, "SMS envoyé depuis le PC")
    check(phone.expect("kdeconnect.sms.request", where=lambda b: b.get("messageBody") == "Rendez-vous à 8 h"
                       and b.get("addresses") == [{"address": "+33612345678"}]) is not None,
          "le téléphone reçoit le SMS à envoyer")
    check(cli("thread", phone.id, "3")[0] == 0 and phone.expect("kdeconnect.sms.request_conversation",
          where=lambda b: b.get("threadID") == 3) is not None, "historique d'une conversation demandé")

    # Fichier du téléphone vers le PC (nom piégé).
    content = os.urandom(300_000)
    upload = socket.socket()
    upload.bind(("127.0.0.1", 0))
    upload.listen(1)

    def serve_file():
        connection, _ = upload.accept()
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(str(phone.cert), str(phone.key))
        tls = context.wrap_socket(connection, server_side=True)
        tls.sendall(content)
        tls.close()
    threading.Thread(target=serve_file, daemon=True).start()
    phone.send("kdeconnect.share.request", {"filename": "../../.bashrc/photo.jpg", "lastModified": 1700000000000},
               payload_size=len(content), port=upload.getsockname()[1])
    received = work / "home/Téléchargements/photo.jpg"
    wait(lambda: received.exists() and received.read_bytes() == content, "fichier reçu dans Téléchargements, nom assaini")
    check(int(received.stat().st_mtime) == 1700000000, "date du fichier conservée")
    check(not any(p.name.endswith(".part") for p in received.parent.iterdir()), "aucun fichier partiel laissé")
    phone.send("kdeconnect.share.request", {"url": "https://example.org/recette"})
    wait(lambda: json.loads((work / f"data/{phone.id}/received.json").read_text())[-1]["text"] == "https://example.org/recette",
         "lien reçu listé")

    # Fichier du PC vers le téléphone.
    outgoing = work / "rapport.pdf"
    outgoing.write_bytes(os.urandom(500_000))
    check(cli("send", phone.id, str(outgoing))[0] == 0, "« Envoyer un fichier » accepté")
    offer = phone.expect("kdeconnect.share.request", where=lambda b: b.get("filename") == "rapport.pdf")
    check(offer is not None and offer.get("payloadSize") == 500_000, "le téléphone reçoit l'offre avec la taille")
    if offer:
        raw = socket.create_connection(("127.0.0.1", offer["payloadTransferInfo"]["port"]), timeout=10)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
        context.check_hostname = False
        context.verify_mode = ssl.CERT_NONE
        context.load_cert_chain(str(phone.cert), str(phone.key))
        tls = context.wrap_socket(raw)
        check(tls.getpeercert(binary_form=True) == phone.winlux_der, "transfert chiffré avec le certificat de Winlux")
        data = b""
        while len(data) < 500_000:
            chunk = tls.recv(65536)
            if not chunk:
                break
            data += chunk
        check(data == outgoing.read_bytes(), "fichier reçu intact par le téléphone")

    # Batterie, sonnerie.
    phone.send("kdeconnect.battery", {"currentCharge": 42, "isCharging": True, "thresholdEvent": 0})
    wait(lambda: device(phone.id)["battery"] == {"level": 42, "charging": True}, "batterie du téléphone affichée")
    check(cli("ring", phone.id)[0] == 0 and phone.expect("kdeconnect.findmyphone.request") is not None, "« Faire sonner »")

    # Imposteur : même identifiant, autre certificat.
    impostor = FakePhone("Pixel de test", phone.id, work / "impostor")
    try:
        impostor.connect(phone.winlux_identity["tcpPort"])
        closed = impostor.expect("#closed", timeout=5) is not None
    except (OSError, ssl.SSLError, ValueError):
        closed = True
    check(closed, "un autre certificat pour le même téléphone est refusé")
    phone.send("kdeconnect.battery", {"currentCharge": 41, "isCharging": False})
    wait(lambda: device(phone.id)["battery"]["level"] == 41, "la vraie liaison reste ouverte")

    # Réglages par fonction.
    check(cli("set", phone.id, "sms", "off")[0] == 0, "SMS désactivés pour ce téléphone")
    check(not sms.exists() and not contacts.exists(), "SMS et contacts effacés du PC")
    phone.send("kdeconnect.sms.messages", {"messages": [{"_id": 9, "thread_id": 4, "body": "x", "addresses": [], "date": 1, "type": 1}]})
    time.sleep(0.5)
    check(not sms.exists(), "plus aucun SMS enregistré")
    check(cli("set", phone.id, "sms", "on")[0] == 0 and phone.expect("kdeconnect.sms.request_conversations") is not None,
          "SMS réactivés : conversations redemandées")

    # Dissociation par le téléphone, puis appairage demandé depuis le PC.
    phone.send("kdeconnect.pair", {"pair": False})
    wait(lambda: phone_module.paired_devices() == {} and not (work / f"data/{phone.id}").exists(),
         "dissocié depuis le téléphone : données effacées")
    code, out, _ = cli("pair", phone.id)
    request_packet = phone.expect("kdeconnect.pair", where=lambda b: b.get("pair") is True)
    check(code == 0 and request_packet is not None and abs(request_packet["body"]["timestamp"] - time.time()) < 60,
          "« Associer » depuis le PC : demande horodatée envoyée")
    check(json.loads(out)["key"] == phone_module.verification_key(phone.der, phone.winlux_der, request_packet["body"]["timestamp"]),
          "code affiché sur le PC identique à celui du téléphone")
    phone.send("kdeconnect.pair", {"pair": True})
    wait(lambda: phone.id in phone_module.paired_devices(), "le téléphone accepte : associé")
    check(cli("unpair", phone.id)[0] == 0 and phone.expect("kdeconnect.pair", where=lambda b: b.get("pair") is False) is not None,
          "« Dissocier » sur le PC prévient le téléphone")
    check(phone_module.paired_devices() == {}, "plus aucun téléphone associé")

    # Demande hors délai d'horloge refusée.
    phone.send("kdeconnect.pair", {"pair": True, "timestamp": int(time.time()) - 7200})
    check(phone.expect("kdeconnect.pair", where=lambda b: b.get("pair") is False) is not None,
          "demande d'appairage avec une horloge trop décalée refusée")

    # Réseau public : aucune annonce, aucune liaison.
    (work / "firewall.conf").write_text("profile=public\n")
    time.sleep(2.5)
    phone2 = FakePhone("Autre", "b1b2c3d4e5f60718293a4b5c6d7e8f91", work / "phone2")
    try:
        phone2.connect(phone.winlux_identity["tcpPort"])
        refused = phone2.expect("#closed", timeout=5) is not None
    except (OSError, ssl.SSLError, ValueError):
        refused = True
    check(refused and status()["public_network"], "réseau public : liaison refusée et signalée")
    (work / "firewall.conf").write_text("profile=private\n")

    check(cli("stop")[0] == 0, "arrêt du service")
    daemon.wait(timeout=10)
    check(not (work / "run/control.sock").exists(), "socket de contrôle retiré à l'arrêt")
finally:
    if daemon.poll() is None:
        daemon.kill()

# Pare-feu : ports du téléphone ouverts sur les réseaux privés seulement.
root = work / "root"
(root / "etc/winlux").mkdir(parents=True)
(root / "etc/winlux/firewall.conf").write_text("profile=private\nrule=12\tWeb\tin\tallow\ttcp\t80\t\tany\t1\n")
control = dict(os.environ, WINLUX_CONTROL_ROOT=str(root), WINLUX_CONTROL_NO_STATUS="1")
subprocess.run(["sh", str(HERE / "winlux-control.sh"), "phone-firewall-on"], env=control, check=True,
               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
rules = (root / "etc/winlux/firewall.conf").read_text()
check("rule=9447\tTelephone connecte Winlux (TCP)\tin\tallow\ttcp\t1716,1739-1764\t\tprivate,domain\t1" in rules
      and "rule=9448\tTelephone connecte Winlux (UDP)\tin\tallow\tudp\t1716\t\tprivate,domain\t1" in rules,
      "pare-feu : TCP 1716 + 1739-1764 et UDP 1716, réseaux privés et de domaine")
check("rule=12\tWeb" in rules, "pare-feu : les autres règles sont gardées")
subprocess.run(["sh", str(HERE / "winlux-control.sh"), "phone-firewall-off"], env=control, check=True,
               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
rules = (root / "etc/winlux/firewall.conf").read_text()
check("9447" not in rules and "9448" not in rules and "rule=12" in rules, "pare-feu : règles retirées à la désactivation")
root_policy = SourceFileLoader("winlux_root", str(HERE / "winlux-root.py")).load_module()
check(root_policy.control_level(["phone-firewall-on"]) == "user", "relais root : règle du téléphone sans élévation")

# Paramètres : page « Appareils mobiles » masquée sans téléphone associé.
try:
    import gi  # noqa: F401
    gi.require_version("Gtk", "3.0")
    pages = SourceFileLoader("winlux_settings_pages", str(HERE / "winlux-settings-pages.py")).load_module()
    check(not pages.page_available(pages.PAGE_PHONE, {}), "Paramètres : page masquée sans téléphone associé")
    devices = work / "config/devices.json"
    devices.write_text(json.dumps({phone.id: {"name": "Pixel", "cert": base64.b64encode(phone.der).decode()}}))
    check(pages.page_available(pages.PAGE_PHONE, {}), "Paramètres : page visible avec un téléphone associé")
except (ImportError, ValueError) as error:
    print("     (Paramètres GTK non vérifiés ici :", error, ")")

if failures:
    print(f"\n{len(failures)} échec(s). Journal du service :")
    print((work / "service.log").read_text(errors="replace")[-4000:])
    sys.exit(1)
print("\nTéléphone connecté : tout est bon.")
