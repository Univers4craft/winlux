#!/usr/bin/python3
"""Certificats et heure (winlux-certificates).

Une petite autorité de test (openssl) signe des certificats de serveur ; des
serveurs TLS locaux jouent un site correct, un site d'un autre nom et un site
inconnu ; un faux portail captif (http) et un faux serveur de temps (SNTP)
complètent le réseau. Vérifie : ajout d'une autorité (PEM, DER, refus d'un
.p12 ou d'un fichier quelconque), retrait, magasin reconstruit sans perdre
les autorités d'origine, politique Firefox, diagnostic lisible dans les
quatre langues (date fausse, expiré, autorité inconnue, nom, portail),
synchronisation de l'heure qui rejette une réponse forgée, relais root, et
la page Certificats de Paramètres (sous Xvfb si DISPLAY est là).
"""
import http.server
import json
import os
from pathlib import Path
import runpy
import shutil
import socket
import socketserver
import ssl
import struct
import subprocess
import sys
import tempfile
import threading
import time

HERE = Path(__file__).resolve().parent
TOOL = HERE / "winlux-certificates.py"
failures = []


def ok(condition, message):
    print(("ok   " if condition else "ÉCHEC ") + message)
    if not condition: failures.append(message)


work = Path(tempfile.mkdtemp(prefix="winlux-certs-"))
pki = work / "pki"
pki.mkdir()


def openssl(*args):
    subprocess.run(["openssl", *args], check=True, capture_output=True, cwd=pki)


openssl("req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "ca.key", "-out", "ca.pem", "-days", "30",
        "-subj", "/CN=Lycée Winlux Racine/O=Académie de test", "-addext", "basicConstraints=critical,CA:TRUE",
        "-addext", "keyUsage=critical,keyCertSign,cRLSign")
openssl("x509", "-in", "ca.pem", "-outform", "DER", "-out", "ca.der")


def server_cert(name, host):
    openssl("req", "-newkey", "rsa:2048", "-nodes", "-keyout", f"{name}.key", "-out", f"{name}.csr", "-subj", f"/CN={host}")
    # Extensions exigées par OpenSSL récent (mode strict de Python 3.13).
    (pki / f"{name}.ext").write_text(f"subjectAltName=DNS:{host}\nauthorityKeyIdentifier=keyid,issuer\n"
                                    "keyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n")
    openssl("x509", "-req", "-in", f"{name}.csr", "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial",
            "-out", f"{name}.pem", "-days", "30", "-extfile", f"{name}.ext")


server_cert("localhost", "localhost")
server_cert("autre", "autre.exemple")
(pki / "alice.p12").write_bytes(b"0\x82")
(pki / "texte.crt").write_text("ceci n'est pas un certificat\n")

# Magasin du système factice : 3 autorités « d'origine » prises au vrai magasin.
real = Path("/etc/ssl/certs/ca-certificates.crt").read_text()
origin = "".join(block + "-----END CERTIFICATE-----\n" for block in real.split("-----END CERTIFICATE-----\n")[:3])
sys_root = work / "root"
(sys_root / "etc/ssl/certs").mkdir(parents=True)
bundle = sys_root / "etc/ssl/certs/ca-certificates.crt"
bundle.write_text(origin)


# ── Serveurs locaux ─────────────────────────────────────────────────────────

def tls_server(cert):
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(pki / f"{cert}.pem", pki / f"{cert}.key")
    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(8)

    def serve():
        while True:
            client, _ = listener.accept()
            try:
                with context.wrap_socket(client, server_side=True) as tls: tls.recv(1)
            except (ssl.SSLError, OSError):
                pass
    threading.Thread(target=serve, daemon=True).start()
    return listener.getsockname()[1]


good_port = tls_server("localhost")
other_port = tls_server("autre")


class Portal(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(302)
        self.send_header("Location", "http://portail.gare/login")
        self.end_headers()

    def log_message(self, *args): pass


portal = socketserver.TCPServer(("127.0.0.1", 0), Portal)
threading.Thread(target=portal.serve_forever, daemon=True).start()


class Example(Portal):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"<h1>Example Domain</h1>")


example = socketserver.TCPServer(("127.0.0.1", 0), Example)
threading.Thread(target=example.serve_forever, daemon=True).start()


def ntp_server(offset, forge=False):
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("127.0.0.1", 0))

    def serve():
        while True:
            data, address = sock.recvfrom(512)
            now = time.time() + offset + 2208988800
            stamp = struct.pack("!II", int(now), int((now % 1) * 2 ** 32))
            origin = b"\0" * 8 if forge else data[40:48]
            sock.sendto(bytes([0x24, 2, 0, 0]) + b"\0" * 20 + origin + stamp + stamp, address)
    threading.Thread(target=serve, daemon=True).start()
    return sock.getsockname()[1]


bin_dir = work / "bin"
bin_dir.mkdir()
socket_path = work / "root.sock"
base = dict(os.environ, WINLUX_SYS_ROOT=str(sys_root), WINLUX_CA_BUNDLE=str(bundle), WINLUX_CERT_RUN=str(work / "run"),
            WINLUX_LOCAL_CA=str(sys_root / "usr/local/share/ca-certificates/winlux"), WINLUX_CA_MANUAL="1",
            WINLUX_PORTAL_PROBE=f"http://127.0.0.1:{example.server_address[1]}/", WINLUX_LANG="fr",
            HOME=str(work / "home"), PATH=f"{bin_dir}:{os.environ['PATH']}", WINLUX_ROOT_SOCKET=str(socket_path))


def run(*args, **extra):
    env = dict(base, **extra)
    result = subprocess.run([sys.executable, "-B", str(TOOL), *args], capture_output=True, text=True, env=env, timeout=60)
    try: return json.loads(result.stdout.strip().splitlines()[-1])
    except (IndexError, ValueError): return {"ok": False, "raw": result.stdout + result.stderr}


# ── Ajout et retrait d'une autorité ─────────────────────────────────────────
status = run("status")
ok(status.get("ok") and status.get("system") == 3 and status.get("added") == [], f"état : magasin d'origine, rien d'ajouté ({status})")
unknown = run("check", f"localhost:{good_port}")
ok(unknown.get("error") == "unknown-authority" and "Certificats" in unknown.get("message", ""),
   f"site signé par une autorité inconnue : message clair qui renvoie vers Paramètres ({unknown.get('error')})")
added = run("add", str(pki / "ca.pem"))
ok(added.get("ok") and added.get("subject") == "Lycée Winlux Racine", f"autorité PEM ajoutée ({added})")
ok(bundle.read_text().count("BEGIN CERTIFICATE") == 4 and bundle.read_text().startswith(origin),
   "magasin reconstruit : autorités d'origine gardées, la nouvelle ajoutée")
policy = json.loads((sys_root / "etc/firefox/policies/policies.json").read_text())
ok(any(path.endswith("Lycee-Winlux-Racine.crt") for path in policy["policies"]["Certificates"]["Install"]),
   f"Firefox reçoit l'autorité par sa politique d'entreprise ({policy})")
links = [p.name for p in (sys_root / "etc/ssl/certs").iterdir()]
ok("winlux-Lycee-Winlux-Racine.pem" in links and any(name.endswith(".0") for name in links),
   f"/etc/ssl/certs : lien et empreinte pour le Wi-Fi d'entreprise ({links})")
trusted = run("check", f"localhost:{good_port}")
ok(trusted.get("ok") and "fonctionne" in trusted.get("message", ""), f"même site après l'ajout : connexion acceptée ({trusted})")
again = run("add", str(pki / "ca.der"))
ok(again.get("ok") and len(run("status")["added"]) == 1 and bundle.read_text().count("BEGIN CERTIFICATE") == 4,
   "même autorité en DER : reconnue, pas de doublon")
mismatch = run("check", f"localhost:{other_port}")
ok(mismatch.get("error") == "name-mismatch" and "interceptée" in mismatch.get("message", ""),
   f"certificat d'un autre nom : refusé, message d'alerte ({mismatch.get('error')})")
personal = run("add", str(pki / "alice.p12"))
ok(personal.get("error") == "personal" and "Wi-Fi" in personal.get("message", ""), "un .p12 renvoie vers la page Wi-Fi")
garbage = run("add", str(pki / "texte.crt"))
ok(garbage.get("error") == "not-certificate", "fichier qui n'est pas un certificat refusé")
missing = run("add", str(pki / "absent.crt"))
ok(missing.get("error") == "unreadable", "fichier introuvable refusé")
relative = run("add", "ca.pem")
ok(not relative.get("ok"), "chemin relatif refusé")
removed = run("remove", added["added"][0])
ok(removed.get("ok") and bundle.read_text() == origin.rstrip("\n") + "\n" or bundle.read_text().count("BEGIN CERTIFICATE") == 3,
   f"autorité retirée : magasin d'origine retrouvé ({removed})")
ok("Install" not in json.loads((sys_root / "etc/firefox/policies/policies.json").read_text())["policies"].get("Certificates", {}),
   "Firefox : autorité retirée de la politique")
ok(not run("remove", "../../etc/passwd").get("ok"), "retrait : nom avec chemin refusé")

# ── Diagnostic : date fausse, portail, expiré, inconnu ──────────────────────
clock = run("check", f"localhost:{good_port}", WINLUX_CLOCK_NOW="315532800")
ok(clock.get("error") == "clock" and "date du système est fausse" in clock.get("message", ""),
   f"date en 1980 : « la date du système est fausse » ({clock.get('message')})")
portal_answer = run("check", f"localhost:{good_port}", WINLUX_PORTAL_PROBE=f"http://127.0.0.1:{portal.server_address[1]}/")
ok(portal_answer.get("error") == "portal" and "page de connexion" in portal_answer.get("message", ""),
   f"Wi-Fi public qui intercepte : portail expliqué ({portal_answer.get('error')})")
module = runpy.run_path(str(TOOL), run_name="certificates_test")
classify = module["classify"]
ok(classify(10, False, False) == "expired" and classify(10, True, False) == "clock" and classify(9, False, False) == "clock"
   and classify(20, False, False) == "unknown-authority" and classify(62, False, False) == "name-mismatch"
   and classify(62, False, True) == "portal", "codes OpenSSL → expiré, date, autorité inconnue, nom, portail")
dns = run("check", "nom-qui-n-existe-pas.invalid")
ok(dns.get("error") == "dns", "nom introuvable dit en clair")
closed = socket.socket(); closed.bind(("127.0.0.1", 0)); closed_port = closed.getsockname()[1]; closed.close()
down = run("check", f"localhost:{closed_port}")
ok(down.get("error") == "unreachable", "serveur muet dit en clair")
explained = run("explain-curl", "60", f"https://localhost:{good_port}/x")
ok("autorité" in explained.get("message", ""), f"mises à jour : échec curl 60 expliqué ({explained})")
ok(run("explain-curl", "6", "https://github.com/").get("message") == "", "autre échec curl : laissé au message réseau")
for lang in ("en", "es", "de"):
    os.environ["WINLUX_LANG"] = lang
    localized = runpy.run_path(str(TOOL), run_name=f"certificates_{lang}")
    texts = {code: localized["message"]({"ok": code == "ok", "code": code, "error": code, "host": "h"})
             for code in localized["MESSAGES"]}
    ok(all(text != module["MESSAGES"][code].format(host="h", date="") for code, text in texts.items()),
       f"{lang} : chaque message traduit")
os.environ["WINLUX_LANG"] = "fr"
missing_tr = [text for text in module["MESSAGES"].values() if text not in module["_TR"]]
ok(not missing_tr, f"tous les messages dans les quatre langues ({missing_tr})")

# ── Heure : SNTP, réponse forgée rejetée, réglage ───────────────────────────
good_ntp = ntp_server(120.0)
forged_ntp = ntp_server(-3600.0, forge=True)
synced = run("time", "sync", WINLUX_NTP_SERVERS="127.0.0.1", WINLUX_NTP_PORT=str(good_ntp), WINLUX_CLOCK_NO_SET="1")
ok(synced.get("ok") and abs(synced.get("offset", 0) - 120) < 1, f"SNTP : décalage de 2 minutes mesuré ({synced})")
state = json.loads((work / "run/time-sync.json").read_text())
ok(state.get("last") and state.get("server") == "127.0.0.1", "SNTP : dernière synchronisation notée")
forged = run("time", "sync", WINLUX_NTP_SERVERS="127.0.0.1", WINLUX_NTP_PORT=str(forged_ntp), WINLUX_CLOCK_NO_SET="1")
ok(not forged.get("ok") and forged.get("error") == "no-server", f"SNTP : réponse forgée (mauvais nonce) rejetée ({forged.get('error')})")
ok(run("status").get("time_auto") is True, "heure automatique par défaut")
off = run("time", "auto", "off")
ok(off.get("ok") and "AUTO=0" in (sys_root / "etc/winlux/time.conf").read_text() and run("status").get("time_auto") is False,
   "heure automatique désactivable")
run("time", "auto", "on")

# ── Relais root ─────────────────────────────────────────────────────────────
root_tool = runpy.run_path(str(HERE / "winlux-root.py"), run_name="certificates_root_test")
level = root_tool["exec_level"]
for args, expected in ((["winlux-certificates", "add", "/home/moi/ca.crt"], "admin"),
                       (["winlux-certificates", "remove", "Lycee-Winlux-Racine"], "admin"),
                       (["winlux-certificates", "time", "auto", "off"], "admin"),
                       (["winlux-certificates", "time", "sync"], "user"),
                       (["winlux-certificates", "add", "ca.crt"], None),
                       (["winlux-certificates", "remove", "../x"], None),
                       (["winlux-certificates", "time-daemon"], None),
                       (["winlux-certificates", "sync-user"], None)):
    ok(level(args) == expected, f"relais root : {' '.join(args)} → {expected or 'refusé'}")

# ── Branchements ────────────────────────────────────────────────────────────
ok("winlux-certificates time-daemon" in (HERE / "winlux-system-services.sh").read_text(), "démarrage : heure réglée par les services système")
ok("winlux-certificates" in (HERE / "winlux-session-startup.py").read_text(), "ouverture de session : navigateurs NSS synchronisés")
ok(" winlux-certificates" in (HERE / "stage-desktop-components.sh").read_text().split("winlux-root winlux-wifi", 1)[1].split("; do", 1)[0], "winlux-certificates installé dans l'image")
ok("certificate_error" in (HERE / "winlux-update-apt.py").read_text(), "mises à jour : échec TLS expliqué")
if shutil.which("certutil"):
    run("add", str(pki / "ca.pem"))
    synced_user = run("sync-user")
    listing = subprocess.run(["certutil", "-L", "-d", f"sql:{work / 'home/.pki/nssdb'}"], capture_output=True, text=True).stdout
    ok(synced_user.get("ok") and "Winlux - Lycee-Winlux-Racine" in listing, f"Chrome/Edge (NSS) : autorité importée ({listing.strip()[-120:]})")
    run("remove", "Lycee-Winlux-Racine")
    run("sync-user")
    listing = subprocess.run(["certutil", "-L", "-d", f"sql:{work / 'home/.pki/nssdb'}"], capture_output=True, text=True).stdout
    ok("Winlux - Lycee" not in listing, "NSS : autorité retirée aussi")

# ── Page de Paramètres ──────────────────────────────────────────────────────
if os.environ.get("DISPLAY"):
    for key in ("WINLUX_SYS_ROOT", "WINLUX_CA_BUNDLE", "WINLUX_CERT_RUN", "WINLUX_LOCAL_CA", "WINLUX_CA_MANUAL", "HOME"):
        os.environ[key] = base[key]
    os.environ["WINLUX_CERT_DIRECT"] = "1"
    os.environ["WINLUX_SETTINGS_PAGE"] = "36"
    settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="settings_certificates_test")
    Gtk = settings["Gtk"]
    window = settings["Settings"]()
    ok(window.current == 36 and 36 not in window.hidden_pages, "Paramètres s'ouvre sur la page Certificats")

    def widgets(root):
        if isinstance(root, Gtk.Container):
            for child in root.get_children():
                yield child
                yield from widgets(child)
    names = {w.get_name() for w in widgets(window.body)}
    labels = [w.get_text() for w in widgets(window.body) if isinstance(w, Gtk.Label)]
    ok({"certificates-add", "certificates-address"} <= names, f"page : ajout d'une autorité et test d'un site ({names & {'certificates-add', 'certificates-address'}})")
    ok(any("autorités reconnues" in text for text in labels) and "Aucune autorité ajoutée." in labels,
       "page : nombre d'autorités du système et liste des ajouts")
    panel = window.certificates_panel
    panel.add_file(str(pki / "ca.pem"))
    deadline = time.time() + 20
    while time.time() < deadline and "ajouté" not in panel.message.get_text():
        Gtk.main_iteration_do(False)
        time.sleep(0.02)
    ok("Certificat ajouté : Lycée Winlux Racine." == panel.message.get_text(), f"page : ajout confirmé ({panel.message.get_text()})")
    labels = [w.get_text() for w in widgets(panel.list_box) if isinstance(w, Gtk.Label)]
    ok("Lycée Winlux Racine" in labels, f"page : autorité listée avec son nom ({labels})")
    panel.address.set_text(f"localhost:{other_port}")
    panel.test()
    deadline = time.time() + 20
    while time.time() < deadline and panel.result.get_text() in ("", "Test en cours…"):
        Gtk.main_iteration_do(False)
        time.sleep(0.02)
    ok("le nom ne correspond pas" in panel.result.get_text(), f"page : test d'un site, résultat lisible ({panel.result.get_text()})")
    window.open_page(14)
    ok(getattr(window, "time_switch", None) is not None and window.time_switch.get_active(),
       "Heure et langue : « Régler l’heure automatiquement » proposé et actif")
    window.destroy()

# ── Horloge matérielle, date plancher, nouvel essai rapide ─────────────────
# Portable de damien : pile vide (RTC au 2015-01-02), heure jamais recopiée
# dans la RTC, premier essai réussi après ~4 min → navigateurs refusés.
tool = runpy.run_path(str(TOOL), run_name="winlux_certificates")
import fcntl
calls = []
real_ioctl, real_open = fcntl.ioctl, os.open
fcntl.ioctl = lambda fd, request, data: calls.append((request, data))
tool["write_rtc"].__globals__["RTC_DEVICE"] = os.devnull
ok(tool["write_rtc"](1762196400) is True, "RTC : heure recopiée dans l'horloge matérielle")
request, data = calls[-1]
moment = time.gmtime(1762196400)
ok(request == 0x4024700a and struct.unpack("9i", data)[:6] == (moment.tm_sec, moment.tm_min, moment.tm_hour,
   moment.tm_mday, moment.tm_mon - 1, moment.tm_year - 1900), "RTC : RTC_SET_TIME, struct rtc_time en UTC")
rtc_moment = time.gmtime(1420156800)       # 2015-01-02, la pile du portable
fcntl.ioctl = lambda fd, request, data: struct.pack("9i", rtc_moment.tm_sec, rtc_moment.tm_min, rtc_moment.tm_hour,
                                                     rtc_moment.tm_mday, rtc_moment.tm_mon - 1, rtc_moment.tm_year - 1900, 0, 0, 0)
shown = tool["read_rtc"]()
ok(shown.get("rtc") == "2015-01-02T00:00:00Z" and shown.get("difference", 0) > 10 * 365 * 86400,
   f"time rtc : horloge matérielle lisible sans hwclock ({shown})")
fcntl.ioctl = real_ioctl
# Depuis le compte, /dev/rtc0 est réservé à root : lecture par le relais, sinon refus clair.
g = tool["rtc_status"].__globals__
real_read, real_call, real_socket = g["read_rtc"], g["call"], g["RELAY_SOCKET"]
g["read_rtc"] = lambda: {"ok": False, "error": "no-rtc", "detail": "[Errno 13] Permission denied: '/dev/rtc0'"}
relayed = []
g["call"] = lambda args, timeout=180: relayed.append(args) or {"ok": True, "rtc": "2026-11-03T22:00:00Z"}
g["RELAY_SOCKET"] = os.devnull
real_geteuid, os.geteuid = os.geteuid, lambda: 1000
# Le reste du test force WINLUX_CERT_DIRECT=1 (pas de relais) : ici on teste justement le relais.
real_direct = os.environ.pop("WINLUX_CERT_DIRECT", None)
try:
    ok(tool["rtc_status"]().get("ok") and relayed == [["time", "rtc"]], "time rtc depuis le compte : relais root")
    g["RELAY_SOCKET"] = str(work / "pas-de-relais")
    refused = tool["rtc_status"]()
    ok(refused.get("error") == "admin-only" and "administrateur" in refused.get("message", ""),
       f"time rtc sans relais : « réservé à l’administrateur » ({refused.get('message')})")
finally:
    os.geteuid = real_geteuid
    if real_direct is not None:
        os.environ["WINLUX_CERT_DIRECT"] = real_direct
g["read_rtc"], g["call"], g["RELAY_SOCKET"] = real_read, real_call, real_socket
root_tool = (HERE / "winlux-root.py").read_text(encoding="utf-8")
ok('["time", "sync"], ["time", "rtc"]' in root_tool, "relais root : time rtc autorisé au compte (lecture seule)")
tool["write_rtc"].__globals__["RTC_DEVICE"] = str(work / "absente")
ok(tool["write_rtc"]() is False, "RTC absente : pas d'erreur, juste « non »")
info = work / "BUILD-INFO"
info.write_text("version=0.2.18\nbuilt_at_utc=2026-11-03T17:00:00Z\n")
tool["build_floor"].__globals__["BUILD_INFO"] = info
ok(tool["build_floor"]() == 1793725200, "date plancher : date de construction (UTC)")
settings = []
real_settime = time.clock_settime
glob = tool["apply_floor"].__globals__
import types
fake_time = types.SimpleNamespace(**{name: getattr(time, name) for name in dir(time) if not name.startswith("_")})
fake_time.time = lambda: 1420156800.0       # 2015-01-02, comme la RTC du portable
fake_time.clock_settime = lambda clock, value: settings.append(value)
glob["time"] = fake_time
ok(tool["apply_floor"]() is True and settings == [1793725200], "horloge en 2015 : calée sur la date de construction")
glob["time"] = time
source = TOOL.read_text(encoding="utf-8")
daemon = source[source.index("def time_daemon"):source.index("# ── Certificats")]
ok("apply_floor()" in daemon and "300 if synced else 30" in daemon,
   "démon : date plancher au lancement, essai toutes les 30 s tant que rien n'a réussi")
sync = source[source.index("def sync_once"):source.index("RTC_DEVICE =")]
ok('state["rtc"] = write_rtc()' in sync, "synchro root réussie : RTC mise à jour (clé rtc)")

if failures:
    print(f"\n{len(failures)} échec(s)")
    sys.exit(1)
print("\nCertificats et heure : tout est bon")
