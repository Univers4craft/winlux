#!/usr/bin/env python3
"""Application Claude Code du menu Démarrer : téléchargement vérifié, lanceur,
refus d'un fichier abîmé et ouverture dans le terminal Winlux."""
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import threading

project = Path(__file__).resolve().parent.parent
source = project / "integration/native/claude-code/winlux-claude-code.py"
spec = importlib.util.spec_from_file_location("claude_code", source)
app = importlib.util.module_from_spec(spec)
spec.loader.exec_module(app)

work = Path(tempfile.mkdtemp(prefix="winlux-claude-test-"))
binary = b"MZ" + os.urandom(3 << 20)
files = {}


def publish(checksum):
    files["/stable"] = b"9.8.7\n"
    files["/9.8.7/manifest.json"] = json.dumps(
        {"version": "9.8.7", "platforms": {"win32-x64": {"checksum": checksum, "size": len(binary)}}}).encode()
    files["/9.8.7/win32-x64/claude.exe"] = binary


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = files.get(self.path)
        if body is None:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass  # téléchargement annulé par le test

    def log_message(self, *_):
        pass


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
os.environ.pop("http_proxy", None)
os.environ.pop("HTTP_PROXY", None)
os.environ["no_proxy"] = "127.0.0.1"
app.BASE = f"http://127.0.0.1:{server.server_port}"
app.DATA = work / "data/winlux/claude-code"
app.BIN = work / "bin"

# Fichier abîmé : refusé, rien d'installé.
publish("0" * 64)
try:
    app.install(lambda *_: None, threading.Event())
    raise SystemExit("une somme SHA-256 fausse aurait dû être refusée")
except RuntimeError as e:
    assert "SHA-256" in str(e), e
assert not (app.DATA / "claude.exe").exists()
assert not list(app.DATA.glob(".claude-*")), "fichier partiel oublié"
assert app.installed_launcher() is None

# Fichier correct : installé, lanceur créé, progression jusqu'à 100 %.
publish(hashlib.sha256(binary).hexdigest())
seen = []
launcher = app.install(lambda f, t: seen.append((f, t)), threading.Event())
assert (app.DATA / "claude.exe").read_bytes() == binary
assert (app.DATA / "version").read_text().startswith("9.8.7")
assert launcher == app.BIN / "claude" and os.access(launcher, os.X_OK)
assert str(app.DATA / "claude.exe") in launcher.read_text()
assert app.installed_launcher() == launcher
assert any(f == 1.0 for f, _ in seen) and any(f is not None and 0 < f < 1 for f, _ in seen), seen

# Annulation : arrêt propre sans fichier partiel.
cancel = threading.Event()
cancel.set()
try:
    app.install(lambda *_: None, cancel)
    raise SystemExit("l'annulation aurait dû arrêter le téléchargement")
except app.Cancelled:
    pass
assert not list(app.DATA.glob(".claude-*"))

# Ouverture : le terminal Winlux reçoit le lanceur dans le dossier personnel.
fake = work / "terminal"
fake.mkdir()
log = work / "terminal.log"
(fake / "winlux-terminal").write_text(f'#!/bin/sh\nprintf "%s\\n" "$@" > "{log}"\n')
(fake / "winlux-terminal").chmod(0o755)
os.environ["PATH"] = f"{fake}:{os.environ['PATH']}"
assert app.open_in_terminal(launcher)
for _ in range(100):
    if log.exists() and log.read_text():
        break
    threading.Event().wait(0.05)
args = log.read_text().splitlines()
assert args[0] == "--cwd" and args[2] == "-e", args
assert str(launcher) in "\n".join(args), args

desktop = (project / "integration/native/applications/winlux-claude-code.desktop").read_text()
assert "Icon=winlux-claude-code" in desktop
assert (project / "integration/native/icons/Winlux/scalable/apps/winlux-claude-code.svg").exists()
assert "/usr/share/winlux/native/claude-code/winlux-claude-code.py" in desktop
server.shutdown()
print("Claude Code : installation vérifiée, fichier abîmé refusé, ouverture dans le terminal OK.")
