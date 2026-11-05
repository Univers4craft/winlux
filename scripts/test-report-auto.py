#!/usr/bin/python3
"""Tests hors ligne de l'envoi automatique des plantages (#438) : réglage,
jeton, anonymisation, file persistante, horloge, déduplication, réessai.
GitHub est remplacé par un faux serveur local ; aucun accès à Internet."""
import http.server
import json
import os
from pathlib import Path
import runpy
import tempfile
import threading
import time

tmp = Path(tempfile.mkdtemp())
os.environ.update({
    "WINLUX_REPORT_CONF": str(tmp / "report.conf"),
    "WINLUX_REPORT_DEFAULTS": str(tmp / "defaults.conf"),
    "WINLUX_JOURNAL_DIR": str(tmp / "journal"),
    "WINLUX_RUN_DIR": str(tmp / "run"),
    "XDG_STATE_HOME": str(tmp / "state"),
    "XDG_CONFIG_HOME": str(tmp / "config"),
    "WINLUX_PSTORE": str(tmp / "pstore"),
    "WINLUX_BUILD_INFO": str(tmp / "BUILD-INFO"),
    "WINLUX_TIME_CONF": str(tmp / "time.conf"),
    "WINLUX_REPORT_DELAY": "0",
})
for name in ("journal", "run", "pstore"):
    (tmp / name).mkdir()
r = runpy.run_path(str(Path(__file__).with_name("winlux-report.py")), run_name="report_auto_test")
g = r["flush"].__globals__
checks = 0


def ok(condition, label):
    global checks
    if not condition:
        raise SystemExit(f"ÉCHEC : {label}")
    checks += 1
    print(f"  OK     {label}")


# ── Réglage : désactivé par défaut, sauf avec un jeton personnel ─────────
ok(r["config"]()["AUTO"] == "default" and not r["auto_enabled"](), "sans jeton ni réglage : envoi coupé")
for bad in ("bonjour", "ghp_court", ""):
    try:
        r["set_token"](bad)
        ok(False, f"jeton invalide refusé : {bad!r}")
    except SystemExit:
        pass
ok(not g["USER_TOKEN"].exists(), "jeton invalide : rien d'écrit")
TOKEN = "ghp_" + "A" * 36
r["set_token"](TOKEN + "\n")
ok(g["USER_TOKEN"].stat().st_mode & 0o777 == 0o600, "jeton personnel gardé en 0600")
ok(r["auto_enabled"]() and r["transport"]() == "token", "jeton présent : envoi actif, par l'API")
r["set_value"]("auto=off")
ok(not r["auto_enabled"](), "interrupteur coupé dans Paramètres : rien ne part")
r["set_value"]("auto=on")
ok(r["auto_enabled"](), "case cochée à l'installation : envoi actif")

# ── Anonymisation ─────────────────────────────────────────────────────────
home, values = r["private_values"]()
text = r["redact"]("password=hunter2 psk: 'maison123' token " + TOKEN + " /home/alice/Documents/impots.pdf",
                   home="/home/alice", values={"alice": "<utilisateur>", "LiveboxAlice": "<réseau-wifi>"})
ok("hunter2" not in text and "maison123" not in text and TOKEN not in text, "mots de passe et jetons masqués")
ok("impots" not in text and "alice" not in text, "noms de fichiers personnels et utilisateur masqués")

# ── Collecte : file persistante, chaque plantage une seule fois ───────────
LOG = """[CRASH] ts=100 sig=11 stop=X exception=EXCEPTION_ACCESS_VIOLATION code=0xc0000005 exe=Setup.exe module=setup.exe+0x1a2b pid=42
  path=/home/alice/Téléchargements/Setup.exe
  frame#0 0x0000000140001a2b setup.exe+0x1a2b
[CRASH] ts=200 sig=11 stop=X exception=EXCEPTION_ACCESS_VIOLATION code=0xc0000005 exe=Setup.exe module=setup.exe+0x1a2b pid=43
  path=/home/alice/Téléchargements/Setup.exe
"""
(tmp / "journal/crash.log").write_text(LOG)
ok(r["collect"]() == 2, "deux plantages mis en file")
ok(r["collect"]() == 0 and r["queue_size"]() == 2, "deuxième collecte : rien en double")
item = json.loads(sorted(g["QUEUE"].glob("*.json"))[0].read_text())
ok(item["signature"].startswith("wlx-") and f"`{item['signature']}`" in item["body"], "signature dans le rapport")
ok("alice" not in item["body"] and "Téléchargements" not in item["body"], "rapport en file anonymisé")
ok("urgence:2-confort" in item["labels"] and "Rapport envoyé automatiquement" in item["body"],
   "étiquette d'urgence et mention de l'envoi automatique")
(tmp / "journal/crash.log").unlink()  # arrêt : /run est vidé
ok(r["queue_size"]() == 2, "la file survit à l'arrêt (crash.log perdu)")

# ── Horloge : pas d'HTTPS avec une date fausse ────────────────────────────
(tmp / "BUILD-INFO").write_text("built_at_utc=2026-11-04T05:00:00Z\n")
ok(not r["clock_ready"](now=1420070400), "horloge en 2015 : envoi reporté")
(tmp / "run/time-sync.json").write_text(json.dumps({"last": int(time.time())}))
ok(r["clock_ready"](now=1793800000), "heure synchronisée : envoi permis")

# ── Faux GitHub ───────────────────────────────────────────────────────────
calls = []


class Fake(http.server.BaseHTTPRequestHandler):
    def reply(self, payload):
        data = json.dumps(payload).encode()
        self.send_response(201 if self.command == "POST" else 200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        calls.append(("GET", self.path, self.headers.get("Authorization"), None))
        self.reply({"items": []})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        calls.append(("POST", self.path, self.headers.get("Authorization"), body))
        if self.path.endswith("/comments"):
            self.reply({"html_url": "https://github.com/x/y/issues/7#c"})
        else:
            self.reply({"number": 7, "html_url": "https://github.com/x/y/issues/7"})

    def log_message(self, *args):
        pass


# Réseau absent : la file attend.
g["API"] = "http://127.0.0.1:9"
ok(r["flush"]() is None and r["queue_size"]() == 2, "réseau absent : rien perdu, nouvel essai plus tard")

server = http.server.HTTPServer(("127.0.0.1", 0), Fake)
threading.Thread(target=server.serve_forever, daemon=True).start()
g["API"] = f"http://127.0.0.1:{server.server_port}"
ok(r["flush"]() == 2 and r["queue_size"]() == 0, "réseau revenu : file vidée")
posts = [c for c in calls if c[0] == "POST"]
ok(len(posts) == 1 and posts[0][1].endswith("/issues") and posts[0][2] == f"Bearer {TOKEN}",
   "même plantage deux fois : une seule issue, avec le jeton")
ok(any(c[0] == "GET" and "search/issues" in c[1] for c in calls), "recherche de l'issue existante par signature")
sent = json.loads((g["AUTO_DIR"] / "sent.json").read_text())
sig = item["signature"]
ok(sent[sig]["issue"] == 7 and sent[sig]["pending"] == 1, "seconde occurrence comptée, pas d'envoi en rafale")

# Le lendemain : une nouvelle occurrence devient un commentaire sur #7.
sent[sig]["last"] -= 2 * 86400
(g["AUTO_DIR"] / "sent.json").write_text(json.dumps(sent))
(tmp / "journal/crash.log").write_text(LOG.replace("ts=100", "ts=300").replace("pid=42", "pid=44"))
calls.clear()
ok(r["collect"]() == 1 and r["flush"]() == 1, "nouvelle occurrence envoyée")
ok(len(calls) == 1 and calls[0][1].endswith("/issues/7/comments")
   and "Occurrences depuis le dernier envoi : 2" in calls[0][3]["body"], "commentaire sur l'issue existante")

# Limite par session.
g["_sent_this_run"][0] = g["AUTO_MAX_SENDS"]
(tmp / "journal/crash.log").write_text(LOG.replace("ts=100", "ts=400").replace("pid=42", "pid=45")
                                       .replace("0x1a2b", "0x9000"))
r["collect"]()
ok(r["flush"]() == 0 and r["queue_size"]() == 1, "au plus 5 envois par session : le reste attend")
g["_sent_this_run"][0] = 0

# Interrupteur coupé : la tâche de fond ne collecte ni n'envoie.
r["set_value"]("auto=off")
before = r["queue_size"]()
(tmp / "journal/crash.log").write_text(LOG.replace("ts=100", "ts=500").replace("0x1a2b", "0x5000"))
ok(r["background"](once=True) == 0 and r["queue_size"]() == before, "envoi coupé : tâche de fond inerte")
r["set_value"]("auto=on")
calls.clear()
ok(r["background"](once=True) == 0 and r["queue_size"]() == 0 and calls, "tâche de fond : collecte puis envoi")

# Arrêt anormal du système (pstore) : une issue d'urgence 1, sans fenêtre.
(tmp / "pstore/dmesg-ramoops-0").write_text("<0>[ 12.3] BUG: kernel NULL pointer dereference at 0x18\n")
calls.clear()
r["collect"]()
r["flush"]()
created = [c for c in calls if c[0] == "POST" and c[1].endswith("/issues")]
ok(created and "urgence:1-quotidien" in created[0][3]["labels"] and "wlx-sys-" in created[0][3]["body"],
   "arrêt du système : signature wlx-sys et urgence 1")
ok(r["system_signature"]({"lines": ["<0>[ 99.1] BUG: kernel NULL pointer dereference at 0x20"]})
   == r["system_signature"]({"lines": ["<0>[ 12.3] BUG: kernel NULL pointer dereference at 0x18"]}),
   "signature d'arrêt stable d'un démarrage à l'autre")
ok(r["main"](["winlux-report", "--system"]) == 0, "--system : pas de fenêtre quand l'envoi automatique s'en charge")

# Relais : les étiquettes d'urgence passent.
relay = runpy.run_path(str(Path(__file__).parent.parent / "integration/report-relay/winlux-report-relay.py"),
                       run_name="relay_test")
ok("urgence:2-confort" in relay["ALLOWED_LABELS"], "relais : étiquettes d'urgence autorisées")
server.shutdown()
print(f"winlux-report (envoi automatique) : {checks} vérifications OK")
