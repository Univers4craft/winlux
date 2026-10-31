#!/usr/bin/python3
"""Relais de signalement Winlux : permet d'ouvrir une issue GitHub sans compte.

Le jeton GitHub reste sur ce serveur, jamais dans l'ISO : un jeton embarqué
serait extrait en quelques minutes et servirait à spammer le dépôt (et GitHub
le révoquerait dès sa détection). Le relais n'accepte que des rapports au
format de winlux-report, limite le débit, n'autorise que des étiquettes
connues, ne commente que des issues de signalement, et déduplique par
signature de plantage.

Déploiement (voir docs/SIGNALEMENT-BUGS.md) :
  WINLUX_RELAY_TOKEN_FILE=/etc/winlux-relay/token  (jeton « fine-grained » :
      dépôt Univers4craft/winlux uniquement, permission Issues lecture/écriture)
  winlux-report-relay.py [--port 8787]  derrière un proxy HTTPS (Caddy, nginx…)
Aucune donnée n'est conservée : ni journal des rapports, ni adresse IP au-delà
de la fenêtre de limitation de débit (en mémoire).
"""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
import re
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

REPO = os.environ.get("WINLUX_RELAY_REPO", "Univers4craft/winlux")
API = "https://api.github.com"
ALLOWED_LABELS = {"bug", "signalement-utilisateur", "plantage", "fonction-manquante", "à-trier",
                  "compatibilité-windows", "runtime", "bureau"}
REQUIRED_LABEL = "signalement-utilisateur"
PER_IP_HOUR = int(os.environ.get("WINLUX_RELAY_PER_IP", "5"))
GLOBAL_HOUR = int(os.environ.get("WINLUX_RELAY_GLOBAL", "60"))
SIGNATURE = re.compile(r"Signature : `(wlx-[0-9a-f]{12})`")
MARKERS = ("Winlux", "winlux")

_lock = threading.Lock()
_hits = {}


def token():
    with open(os.environ["WINLUX_RELAY_TOKEN_FILE"]) as handle:
        return handle.read().strip()


def github(method, path, payload=None):
    req = urllib.request.Request(API + path, method=method,
                                 data=json.dumps(payload).encode() if payload is not None else None)
    req.add_header("Accept", "application/vnd.github+json")
    req.add_header("Authorization", f"Bearer {token()}")
    req.add_header("User-Agent", "winlux-report-relay")
    if payload is not None:
        req.add_header("Content-Type", "application/json")
    with urllib.request.urlopen(req, timeout=20) as response:
        return json.loads(response.read().decode() or "{}")


def allow(ip, now=None):
    """Fenêtre glissante d'une heure, par adresse et globale."""
    now = now or time.time()
    with _lock:
        for key in list(_hits):
            _hits[key] = [t for t in _hits[key] if now - t < 3600]
            if not _hits[key]:
                del _hits[key]
        if len(_hits.get(ip, [])) >= PER_IP_HOUR or len(_hits.get("*", [])) >= GLOBAL_HOUR:
            return False
        _hits.setdefault(ip, []).append(now)
        _hits.setdefault("*", []).append(now)
        return True


def validate(data):
    """Renvoie (titre, corps, étiquettes, issue) ou lève ValueError."""
    if not isinstance(data, dict):
        raise ValueError("format")
    title, body = data.get("title"), data.get("body")
    if not isinstance(title, str) or not isinstance(body, str):
        raise ValueError("titre ou corps manquant")
    title, body = title.strip(), body.strip()
    if not 8 <= len(title) <= 256 or not 40 <= len(body) <= 60000:
        raise ValueError("taille")
    if not title.startswith("[") or not any(m in body for m in MARKERS) or "### " not in body:
        raise ValueError("ce n'est pas un rapport winlux-report")
    labels = [l for l in data.get("labels") or [] if isinstance(l, str) and l in ALLOWED_LABELS]
    if REQUIRED_LABEL not in labels:
        labels.append(REQUIRED_LABEL)
    issue = data.get("issue")
    if issue is not None and (not isinstance(issue, int) or issue <= 0):
        raise ValueError("issue")
    # Les mentions notifieraient des tiers : on les neutralise.
    body = re.sub(r"@(?=[A-Za-z0-9-])", "@​", body)
    return title, body, labels, issue


def is_report_issue(number):
    issue = github("GET", f"/repos/{REPO}/issues/{number}")
    return "pull_request" not in issue and any(l.get("name") == REQUIRED_LABEL for l in issue.get("labels", []))


def handle(data):
    title, body, labels, issue = validate(data)
    if issue is None:
        match = SIGNATURE.search(body)
        if match:
            query = urllib.parse.quote(f'repo:{REPO} is:issue label:{REQUIRED_LABEL} "{match.group(1)}"')
            items = github("GET", f"/search/issues?per_page=1&q={query}").get("items", [])
            if items:
                issue = items[0]["number"]
    if issue is not None:
        if not is_report_issue(issue):
            raise ValueError("issue non autorisée")
        comment = github("POST", f"/repos/{REPO}/issues/{issue}/comments", {"body": body})
        return {"url": comment["html_url"], "issue": issue, "deduplicated": data.get("issue") is None}
    created = github("POST", f"/repos/{REPO}/issues", {"title": title, "body": body, "labels": labels})
    return {"url": created["html_url"], "issue": created["number"], "deduplicated": False}


class Handler(BaseHTTPRequestHandler):
    server_version = "winlux-report-relay"

    def reply(self, code, payload):
        data = json.dumps(payload).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        if self.path != "/report":
            return self.reply(404, {"error": "introuvable"})
        ip = self.headers.get("X-Forwarded-For", self.client_address[0]).split(",")[0].strip()
        if not allow(ip):
            return self.reply(429, {"error": "trop de rapports, réessayez plus tard"})
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if length > 80000:
                return self.reply(413, {"error": "rapport trop long"})
            return self.reply(200, handle(json.loads(self.rfile.read(length) or b"{}")))
        except ValueError as error:
            return self.reply(400, {"error": str(error)})
        except (OSError, urllib.error.URLError, KeyError):
            return self.reply(502, {"error": "GitHub indisponible"})

    def log_message(self, *args):
        pass  # aucun journal : ni rapports ni adresses


def main(argv):
    port = int(argv[argv.index("--port") + 1]) if "--port" in argv else 8787
    host = os.environ.get("WINLUX_RELAY_HOST", "127.0.0.1")
    token()  # échoue tout de suite si le jeton manque
    ThreadingHTTPServer((host, port), Handler).serve_forever()


if __name__ == "__main__":
    main(sys.argv)
