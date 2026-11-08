#!/usr/bin/python3
"""Contrôle parental (winlux-parental, #661) : règles, activité, DNS filtrant,
service root et, surtout, un compte surveillé qui ne peut pas lever sa propre
restriction en passant par le relais root.

Tout tourne sans root ni VM : racine factice (WINLUX_PARENTAL_ROOT et
WINLUX_ROOT_PREFIX), appelant du relais simulé, faux résolveur DNS amont.
"""
import datetime
import importlib.util
import json
import os
from importlib.machinery import SourceFileLoader
from pathlib import Path
import re
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time

HERE = Path(__file__).resolve().parent
TOOL = HERE / "winlux-parental.py"
RELAY = HERE / "winlux-root.py"
work = Path(tempfile.mkdtemp(prefix="winlux-parental-test."))
os.environ["WINLUX_PARENTAL_ROOT"] = str(work)
os.environ["WINLUX_LANG"] = "fr"


def load(path, name):
    loader = SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def check(condition, message):
    if not condition:
        print("ERREUR :", message, file=sys.stderr)
        sys.exit(1)
    print("  OK    ", message)


# ── Racine factice : alice administratrice, bob et chloe standard ──────────
(work / "etc/winlux").mkdir(parents=True)
(work / "run/winlux/sessions").mkdir(parents=True)
(work / "etc/passwd").write_text("root:x:0:0:root:/root:/bin/bash\nalice:x:1001:1001:Alice:/home/alice:/bin/bash\n"
                                 "bob:x:1002:1002:Bob:/home/bob:/bin/bash\nchloe:x:1003:1003:Chloé:/home/chloe:/bin/bash\n"
                                 "winlux-web:x:1500:1500::/home/winlux-web:/bin/sh\n")
(work / "etc/group").write_text("sudo:x:27:alice\n")
(work / "etc/winlux/users.conf").write_text("alice:root:shadow\nbob:user:shadow\nchloe:user:shadow\n")
(work / "etc/winlux/live-session").parent.mkdir(parents=True, exist_ok=True)
lib = load(TOOL, "winlux_parental_test")

# ── Traductions : chaque texte affiché existe dans les quatre langues ──────
source = TOOL.read_text(encoding="utf-8")
keys = set(re.findall(r'_t\("((?:[^"\\]|\\.)*)"\)', source)) | set(lib.DAY_NAMES.values())
missing = [key for key in keys if key not in lib._TR or len(lib._TR[key]) != 3 or not all(lib._TR[key])]
check(not missing, f"textes traduits en anglais, espagnol et allemand ({len(keys)})")
braces = [key for key, row in lib._TR.items() if any(set(re.findall(r"{\w+}", t)) != set(re.findall(r"{\w+}", key)) for t in row)]
check(not braces, "mêmes champs {…} dans chaque langue")

# ── Règles de temps ─────────────────────────────────────────────────────────
cfg = lib.validate({"days": {"mon": {"limit": 60, "from": "08:00", "to": "20:00"}}})
monday = datetime.datetime(2026, 11, 9, 10, 0)  # un lundi
check(monday.weekday() == 0, "date d'essai : un lundi")
state = lib.allowance(cfg, {"minutes": 20}, monday)
check(state["allowed"] and state["remaining"] == 40, f"40 minutes restantes sur 60 : {state}")
state = lib.allowance(cfg, {"minutes": 60}, monday)
check(not state["allowed"] and state["reason"] == "limit" and state["until_day"] == "tomorrow", "limite atteinte")
state = lib.allowance(cfg, {"minutes": 0}, monday.replace(hour=7, minute=30))
check(not state["allowed"] and state["reason"] == "schedule" and state["until"] == "08:00", "avant la plage horaire")
state = lib.allowance(cfg, {"minutes": 0}, monday.replace(hour=20, minute=0))
check(not state["allowed"] and state["reason"] == "schedule" and state["until_day"] == "tomorrow", "après la plage horaire")
state = lib.allowance(cfg, {"minutes": 0}, monday.replace(hour=19, minute=50))
check(state["remaining"] == 10, "fin de plage plus proche que la limite")
state = lib.allowance(cfg, {"minutes": 90, "grant_until": monday.timestamp() + 900}, monday)
check(state["allowed"] and state["granted"] and state["remaining"] == 15, "temps accordé par un adulte")
state = lib.allowance(cfg, {"minutes": 0}, monday + datetime.timedelta(days=1))
check(state["allowed"] and state["remaining"] is None, "mardi sans règle : illimité")
for bad in ({"days": {"mon": {"limit": -1}}}, {"days": {"mon": {"limit": True}}},
            {"days": {"mon": {"from": "20:00", "to": "08:00"}}}, {"days": {"mon": {"from": "08:00"}}},
            {"apps": [{"id": "bin:../../bin/sh"}]}, {"apps": [{"id": "bin:openbox"}]},
            {"apps": [{"id": "bin:winlux-settings"}]}, {"web": {"mode": "tout"}},
            {"web": {"mode": "block", "blocked": ["exa mple.com"]}}):
    try:
        lib.validate(bad)
    except ValueError:
        continue
    check(False, f"configuration refusée : {bad}")
check(True, "configurations douteuses refusées (durée, plage, programme du bureau, adresse)")
check(lib.clean_domain("https://www.Exemple.com/page?x") == "exemple.com", "adresse nettoyée")
check(lib.clean_domain("jeux.exemple.fr") == "jeux.exemple.fr", "sous-domaine gardé")
check(lib.clean_domain("pas une adresse") is None, "adresse invalide")

# ── Applications ────────────────────────────────────────────────────────────
check(lib.app_of("/usr/bin/winlux", ["winlux", "C:\\Jeux\\Super Jeu.EXE"]) == "exe:super jeu.exe", "programme Windows")
check(lib.app_of("/usr/lib/firefox/firefox", ["/usr/lib/firefox/firefox", "-contentproc"]) == "bin:firefox", "application Linux")
check(lib.app_of("/usr/bin/python3.12", ["python3", "/usr/bin/winlux-settings"]) == "bin:winlux-settings", "script Python")
check(lib.protected("bin:winlux-settings") and lib.protected("bin:openbox") and not lib.protected("exe:jeu.exe"),
      "programmes du bureau jamais bloqués")

# ── DNS filtrant ────────────────────────────────────────────────────────────
web = {"mode": "block", "blocked": ["jeux.example"], "allowed": [], "safesearch": True}
check(lib.dns_decision("www.jeux.example.", web) == "block", "site bloqué et ses sous-domaines")
check(lib.dns_decision("wikipedia.org", web) == "forward", "autre site résolu")
check(lib.dns_decision("mozilla.cloudflare-dns.com", web) == "block", "DoH connu refusé")
check(lib.dns_decision("use-application-dns.net", {"mode": "off"}) == "block", "signal Mozilla : DoH automatique coupé")
check(lib.dns_decision("www.google.fr", web) == "safe:forcesafesearch.google.com", "Google en recherche protégée")
check(lib.dns_decision("www.youtube.com", web) == "safe:restrict.youtube.com", "YouTube en mode restreint")
allow = {"mode": "allow", "allowed": ["ecole.example"], "blocked": [], "safesearch": False}
check(lib.dns_decision("cours.ecole.example", allow) == "forward" and lib.dns_decision("autre.example", allow) == "block",
      "liste blanche")
rules = lib.nft_ruleset({1002: 54002})
check("meta skuid 1002 udp dport 53 redirect to :54002" in rules and "tcp dport 853 reject" in rules
      and "1.1.1.1" in rules and "2606:4700:4700::1111" in rules, "nftables : DNS du compte redirigé, DoT et DoH refusés")
check(lib.nft_ruleset({}) == "", "aucun compte filtré : aucune règle")
# #816 : hors du réseau local, le compte ne joint que les adresses du DNS filtrant.
check("set ok4_1002 { type ipv4_addr; flags timeout; }" in rules and "meta skuid 1002 ip daddr @ok4_1002 accept" in rules
      and "meta skuid 1002 ip6 daddr @ok6_1002 accept" in rules and "192.168.0.0/16" in rules
      and rules.index("@ok4_1002 accept") < rules.index("meta skuid 1002 meta l4proto tcp reject with tcp reset")
      < rules.index("meta skuid 1002 reject\n"),
      "nftables : VPN, proxy ou DoH inconnu injoignables (seules les adresses données par le filtre, #816)")
check(lib.address_script(1002, {"10.0.0.1": 10800, "2001:db8::1": 30}) ==
      "add element inet winlux_parental ok4_1002 { 10.0.0.1 timeout 10800s }\n"
      "add element inet winlux_parental ok6_1002 { 2001:db8::1 timeout 60s }\n", "nftables : adresses ouvertes au compte")
if shutil.which("nft"):
    probe_nft = subprocess.run(["nft", "-c", "-f", "-"], input=rules + lib.address_script(1002, {"10.0.0.1": 60}),
                               capture_output=True, text=True)
    if "Operation not permitted" not in probe_nft.stderr:
        check(probe_nft.returncode == 0, f"nftables accepte les règles ({probe_nft.stderr.strip()})")
for site in ("nordvpn.com", "www.protonvpn.com", "torproject.org", "www.croxyproxy.com"):
    check(lib.dns_decision(site, web) == "block", f"VPN ou proxy grand public refusé : {site}")
check(lib.dns_port(1002) == 54002 and lib.dns_port(999) is None, "port DNS propre au compte")

# Faux résolveur amont : répond 10.0.0.<n> à tout nom.
upstream = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
upstream.bind(("127.0.0.1", 0))
asked = []


def fake_upstream():
    while True:
        try:
            packet, peer = upstream.recvfrom(4096)
        except OSError:
            return
        _ident, name, qtype, end = lib.parse_query(packet)
        asked.append(name)
        answer = b"\xc0\x0c" + struct.pack("!HHIH", 1, 1, 600, 4) + bytes([10, 0, 0, len(asked)])
        upstream.sendto(lib.reply_with(packet, end, 0, answer, 1), peer)


threading.Thread(target=fake_upstream, daemon=True).start()
(work / "etc/resolv.conf").write_text("nameserver 127.0.0.53\nnameserver 127.0.0.1\n")
os.environ["WINLUX_PARENTAL_UPSTREAM_PORT"] = str(upstream.getsockname()[1])
check(lib.upstreams() == [], "résolveur local ignoré (pas de boucle)")
(work / "etc/resolv.conf").write_text("nameserver 127.0.0.1\n")
lib.upstreams = lambda: ["127.0.0.1"]


class FakeDaemon:
    blocked = []
    opened = []

    def resolved(self, user, records):
        self.opened.extend((user, socket.inet_ntop(socket.AF_INET, data)) for _type, _ttl, data in records)

    def web_of(self, user):
        return web

    def site_blocked(self, user, site):
        self.blocked.append(site)


probe = socket.socket()
probe.bind(("127.0.0.1", 0))
port = probe.getsockname()[1]
probe.close()
daemon = FakeDaemon()
lib.DnsFilter(daemon, "bob", port)


def ask(name, tcp=False):
    query = lib.build_query(name, 1)
    if tcp:
        with socket.create_connection(("127.0.0.1", port), timeout=5) as sock:
            sock.sendall(struct.pack("!H", len(query)) + query)
            size = struct.unpack("!H", lib.recv_exact(sock, 2))[0]
            return lib.recv_exact(sock, size)
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.settimeout(5)
        sock.sendto(query, ("127.0.0.1", port))
        return sock.recvfrom(4096)[0]


answer = ask("jeux.example")
check(answer[3] & 0x0F == 3 and daemon.blocked == ["jeux.example"] and not daemon.opened,
      "site bloqué : NXDOMAIN, tentative notée, aucune adresse ouverte")
answer = ask("wikipedia.org")
check(answer[3] & 0x0F == 0 and lib.answer_records(answer)[0][2][:3] == bytes([10, 0, 0]) and asked[-1] == "wikipedia.org",
      "autre site : réponse du résolveur amont")
check(daemon.opened == [("bob", "10.0.0." + str(len(asked)))],
      "adresse du site ouverte au compte avant la réponse (#816)")
answer = ask("www.google.com", tcp=True)
records = lib.answer_records(answer)
check(asked[-1] == "forcesafesearch.google.com" and records and lib.read_name(answer, 12)[0] == "www.google.com",
      "recherche protégée : adresse de forcesafesearch sous le nom demandé (TCP)")

# ── Commandes root sur la racine factice ───────────────────────────────────
env = {**os.environ, "WINLUX_PARENTAL_ROOT": str(work), "WINLUX_PARENTAL_NOW": str(monday.timestamp())}


def tool(*args, stdin=None, peer=None):
    extra = {"WINLUX_PEER_USER": peer} if peer else {}
    result = subprocess.run([sys.executable, "-B", str(TOOL), *args], env={**env, **extra}, input=stdin,
                            capture_output=True, text=True, timeout=30)
    lines = [line for line in result.stdout.splitlines() if line.startswith("{")]
    return result.returncode, json.loads(lines[-1]) if lines else {}, result


code, out, _ = tool("set", "alice", stdin="{}")
check(code == 1 and "administrateur" in out.get("error", ""), "un administrateur ne peut pas être surveillé")
code, out, _ = tool("set", "inconnu", stdin="{}")
check(code == 1, "compte inconnu refusé")
config = {"days": {"mon": {"limit": 60, "from": "08:00", "to": "20:00"}}, "apps": [{"id": "exe:jeu.exe", "name": "Jeu"}],
          "web": {"mode": "block", "blocked": ["jeux.example"], "safesearch": True}}
code, out, _ = tool("set", "bob", stdin=json.dumps(config))
check(code == 0 and out["config"]["days"]["mon"]["limit"] == 60, "configuration de bob enregistrée")
conf = work / "etc/winlux/parental/bob.json"
check(conf.stat().st_mode & 0o777 == 0o600 and conf.parent.stat().st_mode & 0o777 == 0o700,
      "configuration illisible par les comptes (0600 dans un dossier 0700)")
code, out, _ = tool("check-login", "bob")
check(code == 0, "bob peut ouvrir sa session à 10 h")
day = monday.date().isoformat()
(work / f"var/lib/winlux/parental/bob/{day}.json").parent.mkdir(parents=True, exist_ok=True)
lib.save_usage("bob", monday.date(), {"minutes": 60, "apps": {"exe:jeu.exe": 50, "bin:firefox": 10},
                                      "names": {"bin:firefox": "Firefox"}, "blocked_apps": {"exe:jeu.exe": 2},
                                      "blocked_sites": {"jeux.example": 3}})
check((work / "var/lib/winlux/parental").stat().st_mode & 0o777 == 0o700, "activité dans un dossier root 0700")
code, out, _ = tool("check-login", "bob")
check(code == 1 and out["reason"] == "limit", "limite atteinte : session refusée")
code, out, _ = tool("status", peer="bob")
check(out.get("supervised") and out.get("allowed") is False and "days" not in out and "apps" not in out
      and "config" not in out, "status : bob voit son temps, jamais ses réglages")
code, out, _ = tool("report", "bob")
check(out["days"][-1]["minutes"] == 60 and out["apps"][0]["name"] == "jeu" and out["blocked_sites"][0]["count"] == 3,
      "compte rendu : temps du jour, applications, sites bloqués")
code, out, _ = tool("grant", "bob", "30", peer="alice")
check(code == 0 and out["until"] == "10:30", "temps accordé : 30 minutes")
code, out, _ = tool("check-login", "bob")
check(code == 0, "session permise pendant le temps accordé")
code, out, _ = tool("list")
bob = next(a for a in out["accounts"] if a["name"] == "bob")
check(bob["enabled"] and bob["today"]["used"] == 60 and all(a["name"] != "winlux-web" for a in out["accounts"]),
      "liste des comptes : bob surveillé, compte du navigateur absent")

# ── Le relais root : bob ne lève pas sa propre restriction ─────────────────
relay_lib = load(RELAY, "winlux_root_parental_test")
crypt = relay_lib._crypt
(work / "etc/shadow").write_text(f"alice:{crypt('adulte', '$6$sel$')}:19000::::::\n"
                                 f"bob:{crypt('enfant', '$6$sel$')}:19000::::::\n")
mock = work / "control-mock"
log = work / "control.log"
mock.write_text(f"#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\n")
mock.chmod(0o755)
relay_env = {**env, "WINLUX_ROOT_PREFIX": str(work), "WINLUX_ROOT_SOCKET": str(work / "root.sock"),
             "WINLUX_ROOT_CONTROL": str(mock), "WINLUX_ROOT_PARENTAL": str(TOOL), "DISPLAY": ""}
check(relay_lib.parental_level(["status"]) == "user" and relay_lib.parental_level(["get", "bob"]) == "admin-read"
      and relay_lib.parental_level(["set", "bob"]) == "admin" and relay_lib.parental_level(["daemon"]) is None
      and relay_lib.parental_level(["screen", "bob", ":0"]) is None and relay_lib.parental_level(["check-login", "bob"]) is None,
      "relais : politique du contrôle parental")


def start(peer):
    sock = work / "root.sock"
    proc = subprocess.Popen([sys.executable, "-B", str(RELAY), "serve"], env={**relay_env, "WINLUX_ROOT_TEST_PEER": peer})
    for _ in range(100):
        try:
            with socket.socket(socket.AF_UNIX) as probe:
                probe.connect(str(sock))
            return proc
        except OSError:
            time.sleep(0.05)
    proc.kill()
    check(False, "le relais ne démarre pas")


def client(*args, stdin=None):
    result = subprocess.run([sys.executable, "-B", str(RELAY), *args], env=relay_env, input=stdin,
                            capture_output=True, text=True, timeout=60)
    lines = [line for line in result.stdout.splitlines() if line.startswith("{")]
    return result.returncode, json.loads(lines[-1]) if lines else {}, result


before = conf.read_bytes()
server = start("1002:bob")
try:
    code, out, _ = client("exec", "winlux-parental", "status")
    check(code == 0 and out.get("user") == "bob" and out.get("supervised"), "bob lit son temps restant par le relais")
    for args in (["get", "bob"], ["report", "bob"], ["list"]):
        code, out, result = client("exec", "winlux-parental", *args)
        check(code != 0 and not out and "administrateur" in result.stderr, f"bob ne lit pas ses réglages : {' '.join(args)}")
    attempts = (["set", "bob"], ["disable", "bob"], ["forget", "bob"], ["grant", "bob", "120"], ["enable", "chloe"])
    for args in attempts:
        code, out, result = client("exec", "winlux-parental", *args, stdin='{"enabled": false}')
        check(code != 0 and not out, f"bob ne modifie rien : {' '.join(args)}")
    check(conf.read_bytes() == before, "configuration de bob intacte après ses tentatives")
    for args in (["daemon"], ["screen", "bob", ":0"], ["check-login", "bob"]):
        code, _out, _ = client("exec", "winlux-parental", *args)
        check(code != 0, f"commande interne refusée par le relais : {args[0]}")
    code, _out, result = client("control", "tz-Pacific/Kiritimati")
    check(code != 0 and "administrateur" in result.stderr and not log.exists(),
          "bob ne change pas de fuseau horaire pour décaler ses horaires")
    # Hors du temps permis : le bon mot de passe ne déverrouille pas.
    (work / f"var/lib/winlux/parental/bob/{day}.json").unlink()
    lib.save_usage("bob", monday.date(), {"minutes": 60})
    code, _out, _ = client("check", "bob", stdin="enfant\n")
    check(code != 0, "temps écoulé : bob ne déverrouille pas sa session")
    with socket.socket(socket.AF_UNIX) as sock:
        sock.connect(str(work / "root.sock"))
        sock.sendall(b'{"op": "check", "user": "bob", "password": "enfant"}\n')
        answer = json.loads(sock.makefile().readline())
    check(answer.get("parental") == "limit" and answer["ok"] is False,
          "le relais dit pourquoi (contrôle parental), pour l'écran de verrouillage")
    # Un adulte tape son mot de passe (Contrôle de compte) : la modification passe.
    check(client("elevate", "alice", stdin="adulte\n")[0] == 0, "élévation par le mot de passe d'alice")
    code, out, _ = client("exec", "winlux-parental", "grant", "bob", "15")
    check(code == 0 and out.get("ok"), "après confirmation d'un adulte : temps accordé")
    code, _out, _ = client("check", "bob", stdin="enfant\n")
    check(code == 0, "temps accordé : bob déverrouille")
finally:
    server.terminate()
    server.wait(timeout=10)

server = start("1001:alice")
try:
    code, out, _ = client("exec", "winlux-parental", "get", "bob")
    check(code == 0 and out["config"]["apps"][0]["id"] == "exe:jeu.exe", "alice (administratrice) lit sans confirmation")
    code, out, result = client("exec", "winlux-parental", "disable", "bob")
    check(code != 0 and "administrateur" in result.stderr, "modifier demande le Contrôle de compte, même à alice")
    code, _out, result = client("control", "tz-Europe/Berlin")
    check(code == 0, "alice (non surveillée) change de fuseau horaire")
finally:
    server.terminate()
    server.wait(timeout=10)

# ── Service : applications bloquées, écran de blocage et sabotage ──────────
daemon = lib.Daemon()
daemon.configs = {"bob": lib.load_config("bob")}
victim = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)", "C:\\Jeux\\jeu.exe"])
real_app_of, real_pids, real_uid = lib.app_of, lib.pids_of, lib.uid_of
lib.uid_of = lambda name: os.getuid() if name == "bob" else None
lib.pids_of = lambda uids: {victim.pid: os.getuid()} if os.getuid() in uids else {}
lib.app_of = lambda exe, cmdline: "exe:jeu.exe" if "C:\\Jeux\\jeu.exe" in cmdline else real_app_of(exe, cmdline)
daemon.enforce_apps()
check(victim.wait(timeout=5) == -signal.SIGKILL, "application bloquée fermée aussitôt")
usage = lib.load_usage("bob", datetime.date.today())
check(usage["blocked_apps"].get("exe:jeu.exe") == 1, "tentative notée dans l'activité")

# #815 : une copie renommée d'une application bloquée est reconnue.
apps_dir = work / "home/bob/Documents"
apps_dir.mkdir(parents=True)
name_utf16 = "OriginalFilename".encode("utf-16-le") + b"\0\0"
# Structure String de VERSIONINFO : en-tête de 6 octets aligné sur 4, clé, valeur alignée.
game = (b"MZ" + b"\0" * 62 + os.urandom(5000) + struct.pack("<HHH", 52, 8, 1) + name_utf16
        + "jeu.exe".encode("utf-16-le") + b"\0\0" + os.urandom(3000))
check(lib.pe_original_name(game) == "jeu.exe", "nom d'origine lu dans la ressource VERSIONINFO")
check(lib.pe_original_name(b"MZ" + os.urandom(4000)) is None, "pas de VERSIONINFO : pas de nom d'origine")
(apps_dir / "jeu.exe").write_bytes(game)
shutil.copy(apps_dir / "jeu.exe", apps_dir / "devoirs.exe")
(apps_dir / "maths.exe").write_bytes(game.replace(name_utf16, b"X\0" * (len(name_utf16) // 2)))
shutil.copy(apps_dir / "maths.exe", apps_dir / "calcul.exe")
(apps_dir / "dessin.exe").write_bytes(game + b"modifie")
(apps_dir / "lecture.exe").write_bytes(b"MZ" + os.urandom(9000))
check(lib.app_file(os.getpid(), "/usr/bin/winlux", ["winlux", str(apps_dir / "devoirs.exe")]) == str(apps_dir / "devoirs.exe")
      and lib.app_file(os.getpid(), "/usr/bin/winlux", ["winlux", "devoirs.exe"]) == os.path.join(os.getcwd(), "devoirs.exe")
      and lib.app_file(123, "/usr/bin/firefox", ["firefox"]) == "/proc/123/exe", "fichier de l'application d'un processus")
check(lib.file_sample("/dev/null") is None and lib.file_sample(str(apps_dir)) is None,
      "empreinte : jamais un périphérique ni un dossier ouvert en root")
lib.app_of = lambda exe, cmdline: "exe:" + os.path.basename(cmdline[-1]).lower() if cmdline[-1].endswith(".exe") else None
lib.app_file = lambda pid, exe, cmdline: cmdline[-1]


def run_as_bob(name):
    process = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)", str(apps_dir / name)])
    lib.pids_of = lambda uids: {process.pid: os.getuid()} if os.getuid() in uids else {}
    daemon.enforce_apps()
    try:
        return process.wait(timeout=1) == -signal.SIGKILL
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
        return False
    finally:
        lib.pids_of = lambda uids: {}
        daemon.enforce_apps()


check(run_as_bob("devoirs.exe"), "copie renommée d'un .exe bloqué fermée (nom d'origine, #815)")
check(not run_as_bob("maths.exe"), "programme non bloqué laissé ouvert")
daemon.configs["bob"]["apps"].append({"id": "exe:maths.exe", "name": "Maths"})
check(run_as_bob("maths.exe"), "programme bloqué ensuite : fermé par son nom")
check(run_as_bob("calcul.exe"), "copie renommée sans VERSIONINFO fermée par son empreinte (#815)")
check(run_as_bob("dessin.exe"), "copie modifiée gardant son nom d'origine fermée (#815)")
check(not run_as_bob("lecture.exe"), "autre .exe sans rapport laissé ouvert")
prints_file = work / "var/lib/winlux/parental/bob/fingerprints.json"
check(prints_file.stat().st_mode & 0o777 == 0o600 and "exe:maths.exe" in prints_file.read_text(),
      "empreintes gardées par root (0600)")
usage = lib.load_usage("bob", datetime.date.today())
check(usage["blocked_apps"].get("exe:maths.exe", 0) >= 2 and "exe:calcul.exe" not in usage["blocked_apps"],
      "tentative avec une copie notée sous le nom de l'application bloquée")
fresh = lib.Daemon()
mark = fresh.sample(str(apps_dir / "calcul.exe"))[0]
check("exe:maths.exe" in fresh.prints_of("bob").get(mark, []), "empreintes relues au redémarrage du service")
# Application Linux : empreinte prise sur le programme installé, copie reconnue
# dès son premier lancement.
(work / "usr/bin").mkdir(parents=True, exist_ok=True)
(work / "usr/bin/jeu-linux").write_bytes(b"\x7fELF" + os.urandom(6000))
(work / "usr/bin/jeu-linux").chmod(0o755)
shutil.copy(work / "usr/bin/jeu-linux", apps_dir / "devoirs-linux")
fresh.learn_installed("bob", {"apps": [{"id": "bin:jeu-linux", "name": "Jeu Linux"}]})
lib.app_file = lambda pid, exe, cmdline: str(apps_dir / "devoirs-linux")
check(fresh.identify("bob", 99999, "/x", ["devoirs-linux"], "bin:devoirs-linux", {"bin:jeu-linux": "Jeu Linux"})
      == "bin:jeu-linux", "copie renommée d'une application Linux bloquée reconnue dès le premier lancement")
lib.app_of, lib.pids_of = real_app_of, real_pids

opened, frozen = [], []
daemon.sessions = lambda: [("bob", 1002, ":7", True)]
lib.lock_running = lambda uid, display: False
lib.signal_all = lambda uid, sig: frozen.append((uid, sig))
lib.state_of = lambda user, moment=None: {"allowed": False, "reason": "limit", "remaining": None}


class Ended:
    def __init__(self, code):
        self.returncode = code

    def poll(self):
        return self.returncode


def fake_open(user, display, tamper):
    opened.append((user, display, tamper))
    daemon.screens[(user, display)] = {"proc": Ended(None), "tamper": tamper}


daemon.open_screen = fake_open
daemon.enforce_time()
check(opened == [("bob", ":7", 0)], "temps écoulé : écran de blocage ouvert")
daemon.screens[("bob", ":7")]["proc"] = Ended(-9)
daemon.enforce_time()
check(opened[-1] == ("bob", ":7", 1) and not frozen, "écran fermé de force : il revient")
daemon.screens[("bob", ":7")]["proc"] = Ended(1)
daemon.enforce_time()
check(opened[-1] == ("bob", ":7", 2) and frozen == [(1002, signal.SIGSTOP)],
      "fermé une deuxième fois : programmes de bob suspendus")
daemon.screens[("bob", ":7")]["proc"] = Ended(0)
lib.state_of = lambda user, moment=None: {"allowed": True, "remaining": 30}
daemon.enforce_time()
check(frozen[-1] == (1002, signal.SIGCONT) and ("bob", ":7") not in daemon.screens,
      "écran levé normalement : programmes repris")

# ── Paramètres : page 42 branchée ──────────────────────────────────────────
settings = (HERE / "winlux-settings.py").read_text(encoding="utf-8")
pages = (HERE / "winlux-settings-pages.py").read_text(encoding="utf-8")
check('_t("Contrôle parental")]' in settings and '(_t("Comptes"), [33, 38, 42])' in settings
      and '"family-group": 42' in settings, "Paramètres : page 42 dans Comptes, ms-settings:family-group")
check('(42, "Contrôle parental"' in (HERE / "winlux-start-menu.py").read_text(), "recherche du menu Démarrer")
check("PAGE_PARENTAL = 42" in pages and 'PAGE_PARENTAL: "page_parental"' in pages, "page branchée sur winlux-parental")
check("winlux-parental" in (HERE / "stage-desktop-components.sh").read_text()
      and "winlux-parental daemon" in (HERE / "winlux-system-services.sh").read_text(),
      "installé dans /usr/bin et lancé en root au démarrage")

shutil.rmtree(work, ignore_errors=True)
print("OK: contrôle parental (temps, plages, applications, DNS filtrant, relais root, compte surveillé sans échappatoire)")
