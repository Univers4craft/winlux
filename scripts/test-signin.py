#!/usr/bin/python3
"""Options de connexion (#659) : code PIN et empreinte, sans matériel.

Le relais winlux-root tourne sur une racine factice (WINLUX_ROOT_PREFIX) avec
un appelant simulé, winlux-signin derrière lui, et le lecteur d'empreintes
simulé (WINLUX_FPRINT_FAKE) : un fichier « auto » répond à chaque doigt
attendu, son absence fait attendre comme un vrai lecteur.
"""
import json
import os
import runpy
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT_TOOL = HERE / "winlux-root.py"
SIGNIN_TOOL = HERE / "winlux-signin.py"
root_lib = runpy.run_path(str(ROOT_TOOL), run_name="winlux_root_lib")
signin = runpy.run_path(str(SIGNIN_TOOL), run_name="winlux_signin_lib")
checks = 0


def check(condition, message):
    global checks
    if not condition:
        print("ERREUR :", message, file=sys.stderr)
        sys.exit(1)
    checks += 1
    print("  OK  ", message)


# ── Règles du code PIN ──────────────────────────────────────────────────────
problem = signin["pin_problem"]
check(problem("2580") is None and problem("190475") is None, "codes ordinaires acceptés")
check(problem("123") == "pin-format" and problem("12a4") == "pin-format" and problem("1" * 33) == "pin-format",
      "moins de 4 chiffres, lettres, plus de 32 : refusés")
check(problem("1111") == "pin-simple" and problem("1234") == "pin-simple" and problem("9876") == "pin-simple",
      "chiffres répétés ou qui se suivent : refusés")
for lang in ("fr", "en", "es", "de"):
    os.environ["WINLUX_LANG"] = lang
    texts = {code: signin["message"](code) for code in signin["MESSAGES"]}
    check(all(texts.values()) and (lang == "fr" or texts["pin-wrong"] != "Le code PIN est incorrect. Réessayez."),
          f"messages traduits ({lang})")
os.environ["WINLUX_LANG"] = "fr"

# ── Relais sur une racine factice ──────────────────────────────────────────
work = Path(tempfile.mkdtemp(prefix="winlux-signin-test."))
(work / "etc/winlux").mkdir(parents=True)
(work / "run/winlux").mkdir(parents=True)
fake = work / "lecteur"
fake.mkdir()
crypt = root_lib["_crypt"]
(work / "etc/shadow").write_text(f"root:!:19000::::::\nalice:{crypt('motdepasse', '$6$sel$')}:19000::::::\n"
                                 f"bob:{crypt('bob', '$6$sel$')}:19000::::::\ncarol::19000::::::\n")
(work / "etc/winlux/users.conf").write_text("alice:root:shadow\nbob:user:shadow\ncarol:user:shadow\n")
log = work / "control.log"
mock = work / "control-mock"
mock.write_text(f"#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\nexit 0\n")
mock.chmod(0o755)
sock = work / "root.sock"
env = {**os.environ, "WINLUX_ROOT_PREFIX": str(work), "WINLUX_ROOT_SOCKET": str(sock),
       "WINLUX_ROOT_CONTROL": str(mock), "WINLUX_ROOT_SIGNIN": str(SIGNIN_TOOL),
       "WINLUX_FPRINT_FAKE": str(fake), "WINLUX_ROOT_PARENTAL": str(work / "parental-mock")}
# Contrôle parental factice (#661) : tout compte surveillé est hors de ses horaires.
(work / "parental-mock").write_text('#!/bin/sh\necho \'{"supervised": true, "allowed": false, "reason": "hours"}\'\n')
(work / "parental-mock").chmod(0o755)
server = None


def start(peer):
    global server
    if server is not None:
        server.kill()
        server.wait()
    try:
        sock.unlink()
    except FileNotFoundError:
        pass
    server = subprocess.Popen([sys.executable, "-B", str(ROOT_TOOL), "serve"],
                              env={**env, "WINLUX_ROOT_TEST_PEER": peer})
    for _ in range(100):
        try:
            with socket.socket(socket.AF_UNIX) as probe:
                probe.connect(str(sock))
            return
        except OSError:
            time.sleep(0.05)
    check(False, "le relais ne démarre pas")


def ask(action, events=None, timeout=20, **fields):
    return signin["Request"]({"action": action, **fields}, str(sock)).run(
        (lambda item: events.append(item)) if events is not None else None, timeout)


def raw(payload, timeout=20):
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(timeout)
        client.connect(str(sock))
        client.sendall((json.dumps(payload) + "\n").encode())
        lines = client.makefile("r", encoding="utf-8")
        for line in lines:
            answer = json.loads(line)
            if "ok" in answer or "error" in answer:
                return answer
    return {}


try:
    start("1000:alice")
    status = ask("status", user="alice")
    check(status.get("ok") and status["pin"] is False and status["reader"] is True and status["fingers"] == 0
          and status["password"] is True, "état : pas de code, lecteur présent, aucun doigt, mot de passe")

    # Code PIN : mot de passe d'abord, puis règles, puis stockage root seul.
    check(ask("pin-set", user="alice", password="faux", pin="2580").get("error") == "password",
          "créer un code sans le bon mot de passe : refusé")
    check(ask("pin-set", user="alice", password="motdepasse", pin="1234").get("error") == "pin-simple",
          "code trop simple refusé par le relais aussi")
    check(ask("pin-set", user="alice", password="motdepasse", pin="2580").get("ok"), "code PIN créé")
    store = work / "var/lib/winlux/signin"
    pin_file = store / "alice.pin"
    check(stat.S_IMODE(store.stat().st_mode) == 0o700 and stat.S_IMODE(pin_file.stat().st_mode) == 0o600,
          "dossier 0700 et fichier 0600 (root seul)")
    check("2580" not in pin_file.read_text() and json.loads(pin_file.read_text())["n"] >= 2 ** 14,
          "code haché (scrypt, sel), jamais en clair")
    check(ask("status", user="alice")["pin"] is True, "état : code configuré")
    check(ask("pin-check", user="alice", pin="2580").get("ok"), "bon code accepté")
    check(not ask("pin-check", user="alice", pin="0000").get("ok"), "mauvais code refusé")

    # Un autre compte ne teste pas le code d'alice ; il voit seulement l'état.
    start("1001:bob")
    check(ask("pin-check", user="alice", pin="2580").get("ok") is False, "bob ne peut pas essayer le code d'alice")
    check(ask("pin-set", user="alice", password="motdepasse", pin="9999").get("ok") is False,
          "bob ne peut pas changer le code d'alice")
    check(ask("status", user="alice").get("pin") is True, "bob voit l'état d'alice (écran de connexion)")
    # Changer d'utilisateur vers alice avec son code PIN.
    log.write_text("")
    check(raw({"op": "switch", "user": "alice", "pin": "0000"}).get("ok") is False, "bascule : mauvais code refusé")
    check(raw({"op": "switch", "user": "alice", "pin": "2580"}).get("ok") and "session-switch alice" in log.read_text(),
          "bascule vers alice avec son code PIN")

    # Blocage après 5 codes faux, levé par un mot de passe juste.
    start("1000:alice")
    for attempt in range(5):
        answer = ask("pin-check", user="alice", pin="0000")
    check(answer.get("locked") and answer.get("error") == "pin-locked", "5 codes faux : code bloqué")
    check(ask("pin-check", user="alice", pin="2580").get("error") == "pin-locked", "bloqué même avec le bon code")
    check(ask("status", user="alice").get("pin_locked") is True, "état : code bloqué")
    check(raw({"op": "check", "user": "alice", "password": "motdepasse"}).get("ok"), "mot de passe juste")
    check(ask("pin-check", user="alice", pin="2580").get("ok"), "le mot de passe a débloqué le code")

    # Compte sans mot de passe : ni code ni empreinte.
    start("1002:carol")
    check(ask("pin-set", user="carol", password="", pin="2580").get("error") == "no-password",
          "compte sans mot de passe : pas de code PIN")
    check(ask("status", user="carol").get("password") is False, "état : compte sans mot de passe")

    # Empreinte : enregistrement (5 passages du doigt) puis vérification.
    start("1000:alice")
    (fake / "auto").write_text("match")
    events = []
    answer = ask("fp-enroll", events, user="alice", password="motdepasse")
    stages = [e for e in events if e.get("event") == "stage"]
    check(answer.get("ok") and answer.get("finger") == "right-index-finger" and len(stages) == 5
          and stages[-1]["done"] == stages[-1]["total"], "doigt enregistré en 5 passages, progression envoyée")
    check(ask("fp-enroll", user="alice", password="faux").get("error") == "password",
          "enregistrer un doigt sans le bon mot de passe : refusé")
    check(ask("status", user="alice")["fingers"] == 1, "état : un doigt enregistré")
    check(ask("fp-verify", user="alice").get("ok"), "bon doigt : déverrouillé")
    (fake / "auto").write_text("nomatch")
    check(ask("fp-verify", user="alice").get("error") == "no-match", "mauvais doigt : refusé")

    # Doigt attendu, puis l'utilisateur tape son mot de passe : l'attente est
    # annulée, le relais reste disponible et le lecteur est relâché.
    (fake / "auto").unlink()
    request = signin["Request"]({"action": "fp-verify", "user": "alice"}, str(sock))
    result = {}
    thread = threading.Thread(target=lambda: result.update(request.run(None, 90)))
    thread.start()
    time.sleep(1.0)
    started = time.monotonic()
    check(ask("status", user="alice").get("ok") and time.monotonic() - started < 5,
          "pendant l'attente d'un doigt, le relais répond aux autres demandes")
    request.cancel()
    thread.join(10)
    check(not thread.is_alive() and result.get("error") == "cancelled", "attente du doigt annulée")
    time.sleep(1.5)
    leftover = subprocess.run(["pgrep", "-f", f"{SIGNIN_TOOL} fp-verify"], capture_output=True, text=True).stdout.strip()
    check(not leftover, "plus aucun winlux-signin fp-verify après l'annulation")
    # Lecteur simulé « à la main » (VM) : winlux-signin simulate match.
    thread = threading.Thread(target=lambda: result.update(ask("fp-verify", user="alice", timeout=60)))
    result.clear()
    thread.start()
    time.sleep(0.8)
    subprocess.run([sys.executable, "-B", str(SIGNIN_TOOL), "simulate", "match"], env=env, check=True,
                   stdout=subprocess.DEVNULL)
    thread.join(20)
    check(result.get("ok") is True, "simulate match : le doigt posé déverrouille")

    # Bascule vers bob avec son doigt (bob a un doigt enregistré).
    (fake / "bob.fingers").write_text("right-index-finger\n")
    (fake / "auto").write_text("match")
    log.write_text("")
    check(raw({"op": "switch", "user": "bob", "finger": True}).get("ok") and "session-switch bob" in log.read_text(),
          "bascule vers bob avec son doigt")
    (fake / "auto").write_text("nomatch")
    check(raw({"op": "switch", "user": "bob", "finger": True}).get("ok") is False, "bascule : mauvais doigt refusé")

    # Compte surveillé hors de ses horaires : ni code ni doigt ne le déverrouillent.
    (work / "etc/winlux/parental").mkdir(parents=True, exist_ok=True)
    (work / "etc/winlux/parental/alice.json").write_text('{"enabled": true}')
    (fake / "auto").write_text("match")
    check(ask("pin-check", user="alice", pin="2580").get("parental") == "hours",
          "contrôle parental : le bon code PIN ne contourne pas les horaires")
    check(ask("fp-verify", user="alice").get("parental") == "hours",
          "contrôle parental : le bon doigt ne contourne pas les horaires")
    start("1001:bob")
    log.write_text("")
    check(raw({"op": "switch", "user": "alice", "pin": "2580"}).get("parental") == "hours"
          and "session-switch" not in log.read_text(), "contrôle parental : pas de bascule par code PIN")
    start("1000:alice")
    (work / "etc/winlux/parental/alice.json").unlink()

    # Effacer : mot de passe d'abord.
    check(ask("pin-remove", user="alice", password="faux").get("error") == "password", "supprimer le code : mot de passe exigé")
    check(ask("pin-remove", user="alice", password="motdepasse").get("ok") and not pin_file.exists(), "code supprimé")
    check(ask("fp-delete", user="alice", password="motdepasse").get("ok") and ask("status", user="alice")["fingers"] == 0,
          "empreintes supprimées")

    # Sans lecteur : rien à proposer.
    for item in fake.iterdir():
        item.unlink()
    fake.rmdir()
    check(ask("status", user="alice").get("reader") is False, "sans lecteur : la carte empreinte est masquée")
    check(ask("fp-verify", user="alice").get("ok") is False, "sans lecteur : aucune vérification")
finally:
    if server is not None:
        server.kill()

# Compte supprimé : son code et ses empreintes partent avec lui.
with tempfile.TemporaryDirectory() as directory:
    base = Path(directory)
    (base / "etc/winlux").mkdir(parents=True)
    (base / "etc/passwd").write_text("alice:x:1000:1000:Alice:/home/alice:/bin/bash\nbob:x:1001:1001::/home/bob:/bin/bash\n")
    (base / "etc/group").write_text("sudo:x:27:alice\n")
    (base / "etc/winlux/users.conf").write_text("alice:root:shadow\nbob:user:shadow\n")
    accounts_log = base / "log"
    subprocess.run([sys.executable, "-B", str(HERE / "winlux-accounts.py"), "remove", "bob", "keep"],
                   env={**os.environ, "WINLUX_ACCOUNTS_ROOT": str(base), "WINLUX_ACCOUNTS_LOG": str(accounts_log)},
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    check("winlux-signin forget bob" in accounts_log.read_text(), "compte supprimé : code PIN et empreintes effacés")

# Règle polkit : root seul parle à fprintd ; aucune marque dans les textes.
rules = (HERE.parent / "integration/native/fprint/49-winlux-fprint.rules").read_text()
check('subject.user === "root"' in rules and "net.reactivated.fprint." in rules, "polkit : fprintd réservé à root (le relais)")
staging = (HERE / "stage-desktop-components.sh").read_text()
check("49-winlux-fprint.rules" in staging and " winlux-signin " in staging, "règle et winlux-signin dans l'image")
for name in ("winlux-signin.py", "winlux-lock.py", "winlux-settings-pages.py"):
    check("Hello" not in (HERE / name).read_text(), f"aucun nom de marque dans {name}")
print(f"Options de connexion : {checks} vérifications.")
