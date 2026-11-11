#!/usr/bin/python3
"""Tests hors ligne de winlux-report : lecture du journal, cause probable,
masquage, signature, modèle d'issue et réglages. Aucun accès réseau."""
import os
from pathlib import Path
import runpy
import tempfile
import urllib.parse

tmp = Path(tempfile.mkdtemp())
os.environ["WINLUX_REPORT_CONF"] = str(tmp / "report.conf")
os.environ["WINLUX_REPORT_DEFAULTS"] = str(tmp / "defaults.conf")
os.environ["WINLUX_JOURNAL_DIR"] = str(tmp / "journal")
os.environ["WINLUX_RUN_DIR"] = str(tmp / "run")
os.environ["WINLUX_NETWORK_LOG_DIR"] = str(tmp / "netlog")
os.environ["XDG_STATE_HOME"] = str(tmp / "state")
os.environ["WINLUX_PSTORE"] = str(tmp / "pstore")
os.environ["WINLUX_REPORT_ACCOUNT"] = str(tmp / "config/winlux/github-account.json")
(tmp / "journal").mkdir()
(tmp / "run").mkdir()
r = runpy.run_path(str(Path(__file__).with_name("winlux-report.py")), run_name="report_test")

LOG = """[CRASH] ts=100 sig=11 stop=APPLICATION_FAILURE exception=EXCEPTION_ACCESS_VIOLATION code=0x00000000c0000005 exe=Setup.exe module=setup.exe+0x1a2b pid=42
  path=/home/alice/Téléchargements/Setup.exe
  rip=0x0000000140001a2b rsp=0x00007f0000001000 rbp=0x0
  rax=0x0 rcx=0x0 rdx=0x0 addr=0x0000000000000018
  frame#0 0x0000000140001a2b setup.exe+0x1a2b
  frame#1 ?0x0000000140002000 setup.exe+0x2000
[CRASH] ts=200 sig=0 stop=APPLICATION_FAILURE exception=ENTRYPOINT_NOT_FOUND code=0x0 exe=Jeu.exe module=inconnu pid=43
  path=/home/alice/Jeux/Jeu.exe
  detail=fonction=d3d12.dll!D3D12CreateDevice
"""

entries = r["parse_crash_log"](LOG)
assert len(entries) == 2, entries
crash, missing = entries
assert crash["module"] == "setup.exe+0x1a2b" and crash["path"].endswith("Setup.exe")
assert crash["regs"]["addr"] == "0x0000000000000018" and len(crash["frames"]) == 2
assert missing["details"] == ["fonction=d3d12.dll!D3D12CreateDevice"]

# Journal de plusieurs Go (#1903) : seule la fin est lue, sans ligne coupée.
big = tmp / "gros.log"
with open(big, "wb") as stream:
    stream.write(("x" * 99 + "\n").encode() * 30000)
    stream.write("dernière ligne é\n".encode())
tail = r["tail_text"](big, 4096)
assert len(tail.encode()) <= 4096 and tail.endswith("dernière ligne é\n"), tail[-40:]
assert all(len(l) == 99 for l in tail.splitlines()[:-1]), "première ligne coupée écartée"
assert r["tail_text"](big, 10 ** 9).count("\n") == 30001

# Toutes les langues ont toutes les chaînes du français.
for code, table in r["T"].items():
    assert set(table) == set(r["T"]["fr"]), (code, set(r["T"]["fr"]) ^ set(table))

# Cause probable en clair.
journal = ["[warn] kernel32!GetDpiForWindow non implémentée, appel ignoré"]
cause = r["probable_cause"](crash, journal, "fr", cpu_flags=set())
assert "pointeur vide" in cause and "kernel32!GetDpiForWindow" in cause and "setup.exe+0x1a2b" in cause, cause
assert "D3D12CreateDevice" in r["probable_cause"](missing, [], "en")
illegal = dict(crash, exception="EXCEPTION_ILLEGAL_INSTRUCTION", module="winlux+0x10")
text = r["probable_cause"](illegal, [], "de", cpu_flags={"sse4_2"})
assert "AVX2" in text and "Winlux" in text, text

# Signature : stable pour un même emplacement, distincte sinon.
sig = r["signature"](crash)
assert sig.startswith("wlx-") and len(sig) == 16
assert sig == r["signature"](dict(crash, module="SETUP.EXE+0x1a2f", ts="999"))
assert sig != r["signature"](dict(crash, module="setup.exe+0x3000"))
assert r["signature"](missing) != r["signature"](dict(missing, details=["fonction=x!y"]))

# Masquage : chemins, fichiers personnels, identifiants, SSID, série, jetons.
secret = ("Copie /home/alice/Documents/Impôts 2026.pdf et /home/alice/Téléchargements/Setup.exe "
          "C:\\Users\\alice\\Desktop\\notes.txt alice@example.org MAC 12:34:56:78:9a:bc "
          "IP 192.168.1.20 jeton ghp_abcdefghijklmnopqrstuvwxyz0123 Wi-Fi LaBoxDeAlice "
          "série PF3XYZ12 sur portable-alice version 1.0.0.0")
masked = r["redact"](secret, keep_names=["Setup.exe"], home="/home/alice",
                     values={"alice": "<utilisateur>", "LaBoxDeAlice": "<réseau-wifi>",
                             "PF3XYZ12": "<numéro-de-série>", "portable-alice": "<machine>"},
                     keep_strings=["1.0.0.0"])
for leak in ("alice", "Impôts", "notes", "192.168", "ghp_", "LaBox", "PF3XYZ12", "12:34"):
    assert leak not in masked, (leak, masked)
assert "~/…/Setup.exe" in masked and "<fichier>.pdf" in masked and "1.0.0.0" in masked, masked

# Modèle d'issue.
os.environ["WINLUX_LANG"] = "es"
report = r["Report"](crash)
report.exe = {"version": "2.1.0.0"}
title = report.title("Al pulsar Instalar")
assert title.startswith("[Plantage] Setup.exe 2.1.0.0 : EXCEPTION_ACCESS_VIOLATION dans setup.exe"), title
assert "plantage" in report.labels() and "bug" in report.labels()
# #611 : toute issue porte une urgence, avec les noms d'étiquettes du dépôt.
assert "urgence:2-confort" in report.labels(), report.labels()
assert [l for l in r["Report"](None).labels() if l.startswith("urgence:")] == ["urgence:2-confort"]
body = report.body("Al pulsar Instalar", redacted=True)
assert "### Descripción" in body and sig in body and "Al pulsar Instalar" in body
assert "Téléchargements" not in body and "~/…/Setup.exe" in body, body
assert "[Fonction manquante]" in r["Report"](missing).title()
assert r["Report"](None).kind() == "manual" and "à-trier" in r["Report"](None).labels()

# Lien navigateur : borné et étiqueté.
url, truncated = r["browser_url"]("t", "x" * 20000, ["bug", "plantage"])
assert truncated and len(url) <= r["URL_LIMIT"]
url, truncated = r["browser_url"]("Titre é", "court", ["bug"])
query = urllib.parse.parse_qs(urllib.parse.urlparse(url).query)
assert not truncated and query["title"] == ["Titre é"] and query["labels"] == ["bug"]
assert query["template"] == ["rapport-winlux.md"]
# #611 : GitHub ignore les étiquettes du lien pour qui n'est pas membre du
# dépôt ; elles sont recopiées en tête du corps pour le tri.
url, _ = r["browser_url"]("Titre", "corps", ["bug", "à-trier", "urgence:2-confort"])
body_q = urllib.parse.parse_qs(urllib.parse.urlparse(url).query)["body"][0]
assert body_q.startswith("> Étiquettes / Labels : `bug` `à-trier` `urgence:2-confort`\n\ncorps"), body_q
template = (Path(__file__).parent.parent / ".github/ISSUE_TEMPLATE/rapport-winlux.md").read_text()
assert '"à-trier"' in template and '"signalement-utilisateur"' in template

# #609 : la session native n'est pas « XFCE » dans le rapport.
saved_env = {k: os.environ.get(k) for k in ("XDG_CURRENT_DESKTOP", "WINLUX_NATIVE_SHELL", "XDG_SESSION_DESKTOP", "WINLUX_DESKTOP_DEMO")}
os.environ.pop("WINLUX_DESKTOP_DEMO", None)
os.environ.update(XDG_CURRENT_DESKTOP="XFCE", WINLUX_NATIVE_SHELL="1", XDG_SESSION_DESKTOP="winlux")
assert r["session_type"]().startswith("Session native Winlux"), r["session_type"]()
os.environ.pop("WINLUX_NATIVE_SHELL")
assert r["session_type"]().startswith("Session native Winlux")
os.environ.pop("XDG_SESSION_DESKTOP")
assert r["session_type"]().startswith("XFCE")
for key, value in saved_env.items():
    os.environ.pop(key, None) if value is None else os.environ.__setitem__(key, value)

# #608 : l'aperçu n'emploie pas d'émoji couleur (carré vide sans police émoji).
checked = r["Report"](None, language="fr")
checked.checks = [(True, "réseau"), (False, "son")]
text = checked.body("x", redacted=False)
assert "- ✔ réseau" in text and "- ⚠ son" in text and "✅" not in text and "\ufe0f" not in text

# Réglages et choix du transport.
assert r["transport"]() == "browser"
r["set_value"]("mode=file")
r["set_value"]("offer=off")
conf = r["config"]()
assert conf["MODE"] == "file" and conf["OFFER"] == "off" and r["transport"](conf) == "file"
(tmp / "defaults.conf").write_text("RELAY=https://relais.example/\n")
r["set_value"]("mode=auto")
assert r["transport"]() == "relay"
try:
    r["set_value"]("mode=cloud")
    raise AssertionError("valeur refusée attendue")
except SystemExit:
    pass

# Une seule proposition par plantage.
assert r["mark_offered"](crash) and not r["mark_offered"](crash)
(tmp / "journal" / "crash.log").write_text(LOG)
assert r["last_crash"]()["exe"] == "Jeu.exe" and r["last_crash"](300) is None
# Diagnostic : chronologie du programme, vérifications, arrêt du système.
lines = ["2026-10-31T10:00:00.000Z [info] Jeu.exe[43] session: démarrage de /home/alice/Jeux/Jeu.exe",
         "2026-10-31T10:00:05.000Z [warn] Jeu.exe[43] d3d12: kernel32!GetDpiForWindow non implémentée",
         "2026-10-31T10:00:06.000Z [warn] Jeu.exe[43] d3d12: kernel32!GetDpiForWindow non implémentée",
         "2026-10-31T10:00:06.000Z [warn] Autre.exe[44] gdi32: rien à voir",
         "2026-10-31T10:00:07 [warn] d3d9: ancien format sans origine"]
start = r["utc_seconds"]("2026-10-31T10:00:00")
assert start == 1793440800, start
jeu = dict(missing, ts=str(start + 125))
assert [e[2] for e in r["program_events"](jeu, lines)] == ["session", "d3d12", "d3d12"]
sys_events = [(start + 120, "oom", "Out of memory: Killed process 43 (Jeu.exe)")]
st = r["steps"](jeu, lines, sys_events, "fr")
texts = [t for _, t in st]
assert texts[0] == "Lancement de Jeu.exe" and "GetDpiForWindow" in texts[1], texts
assert any(t.startswith("Système : Out of memory") for t in texts), texts
assert "… et 1 autres messages semblables." in texts and texts[-1] == "Le programme a fonctionné 2 min 05 s avant de s'arrêter.", texts
assert r["steps"](None, lines, [], "fr") == []
ck = r["checks"](jeu, lines, sys_events, "llvmpipe (LLVM 17)", "en")
joined = " ".join(t for _, t in ck)
assert "ran out of memory" in joined and "(Jeu.exe)" in joined and "GetDpiForWindow" in joined, joined
assert r["human_size"](28.4 * 1024 ** 3, "fr") == "28,4 Go" and r["human_size"](2 * 1024 ** 3, "en") == "2 GB"
pstore = tmp / "pstore"
assert r["previous_boot_problem"]() is None or r["run"](["journalctl", "-b", "-1", "-q", "-n", "1"])
pstore.mkdir()
(pstore / "dmesg-ramoops-0").write_text("<0>[ 12.3] BUG: kernel NULL pointer dereference\n<0>[ 12.4] Kernel panic - not syncing\n")
problem = r["previous_boot_problem"]()
assert problem and problem["reason"] == "panic" and len(problem["lines"]) == 2, problem
assert r["mark_system_offered"](problem) and not r["mark_system_offered"](problem)
system = r["Report"](None, system=problem, language="fr")
assert system.kind() == "system" and system.title().startswith("[Arrêt du système] BUG: kernel NULL"), system.title()
assert "runtime" in system.labels() and "plantage" in system.labels()
body = system.body("", redacted=True)
assert "Kernel panic" in body and "La session précédente s'est arrêtée brutalement" in body, body
# Relais : seuls des rapports winlux-report passent, sans mention ni étiquette inconnue.
relay = runpy.run_path(str(Path(__file__).parent.parent / "integration/report-relay/winlux-report-relay.py"),
                       run_name="relay_test")
title, body, labels, issue = relay["validate"]({"title": report.title(), "body": body + " @damien",
                                               "labels": ["bug", "admin", "plantage"]})
assert labels == ["bug", "plantage", "signalement-utilisateur", "urgence:2-confort"] and issue is None
_, _, labels, _ = relay["validate"]({"title": report.title(), "body": body,
                                    "labels": ["bug", "urgence:2-confort", "urgence:1-quotidien"]})
assert [l for l in labels if l.startswith("urgence:")] == ["urgence:1-quotidien"], labels
assert "@\u200bdamien" in body
for bad in ({"title": "spam", "body": "x" * 100}, {"title": "[x] titre ok", "body": "pas un rapport " * 10},
            {"title": report.title(), "body": body, "issue": "12"}):
    try:
        relay["validate"](bad)
        raise AssertionError(f"rapport refusé attendu : {bad['title']}")
    except ValueError:
        pass
assert all(relay["allow"]("1.2.3.4", now=1000 + i) for i in range(relay["PER_IP_HOUR"]))
assert not relay["allow"]("1.2.3.4", now=1100) and relay["allow"]("1.2.3.4", now=1000 + 3601 + 10)
# Journaux du réseau (#457) : joints aux signalements manuels, réseaux et
# adresses masqués, absents d'un rapport de plantage d'un .exe.
(tmp / "netlog").mkdir()
(tmp / "netlog/network-manager.log").write_text(
    "<info>  [1730800000.1] device (wlp2s0): state change: config -> need-auth\n"
    "<warn>  [1730800001.2] device (wlp2s0): Activation: (wifi) association took too long\n")
(tmp / "netlog/wpa-supplicant.log").write_text(
    "1730800000.5: wlp2s0: Trying to associate with SSID 'Livebox-Voisin'\n"
    "1730800001.0: wlp2s0: CTRL-EVENT-SSID-TEMP-DISABLED id=0 ssid=\"Livebox-Voisin\" auth_failures=1 reason=WRONG_KEY\n"
    "1730800001.1: wlp2s0: CTRL-EVENT-DISCONNECTED bssid=a4:3e:51:0c:22:9f reason=15\n")
manual = r["Report"](None, language="fr").body("Le Wi-Fi refuse mon mot de passe", redacted=True)
assert "Journal du réseau (NetworkManager)" in manual and "Journal du réseau (wpa_supplicant)" in manual, manual
assert "association took too long" in manual and "reason=WRONG_KEY" in manual
assert "Livebox-Voisin" not in manual and "a4:3e:51" not in manual, manual
assert "Journal du réseau" not in report.body("", redacted=True), "journal du réseau joint à un plantage de .exe"
english = r["Report"](None, language="en").body("", redacted=True)
assert "Network log (wpa_supplicant)" in english
services = (Path(__file__).with_name("winlux-system-services.sh")).read_text(encoding="utf-8")
assert "wpa_supplicant -u -t -f" in services and "wpa_supplicant -u -s" not in services, "wpa_supplicant sans journal"
assert "NetworkManager --no-daemon --debug" in services, "NetworkManager sans journal"
# Journaux de diagnostic joints (#458) : seulement s'ils existent, vides
# signalés, masqués comme le reste, bornés en taille.
diag = tmp / "diag"
r["DIAG_DOMAINS"]  # présent
g = r["diagnostics"].__globals__
g["DIAG_COMMANDS"] = False
os.environ["WINLUX_DIAG_ROOT"] = str(diag)
(diag / "run/winlux").mkdir(parents=True)
(diag / "run/winlux/network.log").write_text(
    "connexion à LiveboxAlice psk=motdepasse1 depuis 192.168.1.20 (aa:bb:cc:dd:ee:ff)\n" * 3)
(diag / "run/winlux/wpa-supplicant.log").write_text("")
entries = r["diagnostics"]("network", "fr")
names = [e[0] for e in entries]
assert names == ["/run/winlux/network.log", "/run/winlux/wpa-supplicant.log"], names
assert entries[1][1] is None and "vide" in entries[1][2], entries[1]
assert r["diagnostics"]("sound", "fr") == []
bare = r["Report"](None, language="fr")
bare.attach = ["network", "sound"]
text = r["redact"]("\n".join(r["diagnostics_markdown"](bare.attach, "fr")), home="/home/alice",
                   values={"LiveboxAlice": "<réseau-wifi>"})
assert "Réseau et Wi-Fi" in text and "aucun journal trouvé" in text, text
assert "LiveboxAlice" not in text and "motdepasse1" not in text and "192.168.1.20" not in text \
    and "aa:bb:cc:dd:ee:ff" not in text, text
assert "wpa-supplicant.log` : vide" in text, text
body = bare.body("", redacted=True)
assert "motdepasse1" not in body and "Journaux de diagnostic — Réseau" in body
bare.attach = []
assert "Journaux de diagnostic" not in bare.body("", redacted=True)
# Journal énorme : raccourci, la fin du rapport (pied de page) reste.
(diag / "run/winlux/network.log").write_text(("x" * 290 + "\n") * 5000)
bare.attach = ["network"]
long_body = bare.body("", redacted=True)
assert len(long_body) <= r["BODY_LIMIT"] and "Signaler un problème" in long_body.splitlines()[-1], long_body[-200:]
for code in r["T"]:
    assert all(k in r["T"][code] for k in ("attach_logs", "d_network", "d_sound", "d_boot", "d_update", "h_diag")), code
# Une clé de traduction définie deux fois dans une langue : la seconde écrase
# la première sans erreur (#558, « Ajouter à #{number} » au lieu du libellé).
import ast
source = (Path(__file__).with_name("winlux-report.py")).read_text()
table = next(n.value for n in ast.parse(source).body
             if isinstance(n, ast.Assign) and getattr(n.targets[0], "id", "") == "T")
for language in table.values:
    keys = [k.value for k in language.keys]
    doubles = {k for k in keys if keys.count(k) > 1}
    assert not doubles, f"clés de traduction en double : {doubles}"
assert r["tr"]("attach_logs", "fr") == "Joindre les journaux de diagnostic :"
assert r["tr"]("attach", "fr", number=12) == "Ajouter à #12"
# #612 : l'état du lien vient de winlux-browser ; Annuler n'est pas « ouvert ».
import time as _time
fake = tmp / "bin"
fake.mkdir(exist_ok=True)
(fake / "winlux-browser").write_text('#!/bin/sh\nread etat < "$FAUX_ETAT"\n'
                                     '[ "$etat" = rien ] || printf "%s\\n" "$etat" > "$WINLUX_BROWSER_STATUS"\n')
(fake / "winlux-browser").chmod(0o755)
os.environ["PATH"] = f"{fake}:{os.environ['PATH']}"
for wanted in ("ouvert", "annule", "attente"):
    (tmp / "etat").write_text(wanted + "\n")
    os.environ["FAUX_ETAT"] = str(tmp / "etat")
    state = r["open_url_watched"]("https://example.invalid/")
    for _ in range(50):
        result = state()
        if result is not None:
            break
        _time.sleep(0.1)
    assert result == wanted, (wanted, result)
assert not list((tmp / "state" / "winlux").glob("navigateur-*")), "fichiers d'état laissés"
# #725 : compte GitHub de l'utilisateur, jeton gardé en 0600, transport manuel.
g = r["connect_token"].__globals__
calls = []


def fake_http(method, url, payload=None, token=None, timeout=20):
    calls.append((method, url, payload, token))
    if url.endswith("/user"):
        return {"login": "alice-dev"} if token == "ghp_" + "a" * 36 else {}
    return {"html_url": "https://github.com/x/y/issues/1"}


real_http = g["http"]
g["http"] = fake_http
g["set_value"]("mode=auto")
(tmp / "defaults.conf").write_text("")
assert g["manual_transport"]() == "browser" and g["account"]() == {}
for bad in ("pas-un-jeton", "ghp_" + "b" * 36):
    try:
        g["connect_token"](bad)
        raise AssertionError("jeton refusé attendu")
    except ValueError:
        pass
assert not (tmp / "config/winlux/github-account.json").exists()
assert g["connect_token"]("  ghp_" + "a" * 36 + "\n") == "alice-dev"
saved = tmp / "config/winlux/github-account.json"
assert saved.stat().st_mode & 0o777 == 0o600, oct(saved.stat().st_mode)
assert g["account"]()["login"] == "alice-dev" and g["manual_transport"]() == "account"
g["set_value"]("mode=browser")
assert g["manual_transport"]() == "browser", "« Par le navigateur » reste respecté"
g["set_value"]("mode=auto")
calls.clear()
url = g["send_api"](g["config"](), "[Plantage] x", "corps", ["bug", "plantage"], None, "account")
method, target, payload, token = calls[-1]
assert target.endswith("/repos/Univers4craft/winlux/issues") and token == "ghp_" + "a" * 36
assert payload["body"].startswith("> Étiquettes / Labels : `bug` `plantage`"), payload["body"]
g["send_api"](g["config"](), "t", "réponse", [], 12, "account")
assert calls[-1][1].endswith("/issues/12/comments") and calls[-1][2] == {"body": "réponse"}
# Le jeton du système reste réservé aux envois automatiques.
assert g["transport"]() == "browser"
g["forget_account"]()
assert g["account"]() == {} and g["manual_transport"]() == "browser"
# Flux « device » : en attente, refus, jeton.
answers = iter([{"error": "authorization_pending"}, {"error": "access_denied"}, {"access_token": "gho_x"}])
g["form_post"] = lambda url, fields: next(answers)
assert g["device_poll"]("Iv1.0123456789", "dc") is None
try:
    g["device_poll"]("Iv1.0123456789", "dc")
    raise AssertionError("refus attendu")
except ValueError as error:
    assert str(error) == "access_denied"
assert g["device_poll"]("Iv1.0123456789", "dc") == "gho_x"
assert g["client_id"]({"GITHUB_CLIENT_ID": "Iv1.0123456789"}) == "Iv1.0123456789"
assert g["client_id"]({"GITHUB_CLIENT_ID": "x y"}) == g["GITHUB_CLIENT_ID"]
# Application « Winlux OS » : identifiant public par défaut, aucun secret.
assert g["client_id"]({}) == "Iv23liGAitONmE9ke1ZT"
sent = []
g["form_post"] = lambda url, fields: sent.append(fields) or {"device_code": "d", "user_code": "ABCD-1234"}
g["device_start"]("Iv23liGAitONmE9ke1ZT")
g["form_post"] = lambda url, fields: sent.append(fields) or {"error": "authorization_pending"}
g["device_poll"]("Iv23liGAitONmE9ke1ZT", "d")
assert all("client_secret" not in f for f in sent) and sent[0] == {"client_id": "Iv23liGAitONmE9ke1ZT"}, sent

# #1049 : confirmation « quoi / où » avant tout envoi.
lines = g["confirm_lines"]("account", "[Bug] Wi-Fi", "x" * 2048, [], True, "alice-dev", "fr")
assert "Univers4craft/winlux" in lines[0] and "@alice-dev" in lines[0], lines
assert "« [Bug] Wi-Fi »" in lines[1] and "2 Ko" in lines[1] and lines[2] == "Aucun journal de diagnostic joint.", lines
lines = g["confirm_lines"]("browser", "t", "b", ["network", "sound"], False, "", "en")
assert "Submit new issue" in lines[0] and lines[2] == "Attached logs: Network and Wi-Fi, Sound." and "not hidden" in lines[3], lines
for language in ("fr", "en", "es", "de"):
    for kind in ("account", "token", "relay", "browser"):
        assert all(g["confirm_lines"](kind, "t", "b", ["boot"], True, "x", language)), (language, kind)
    for key in ("dc_title", "dc_text", "dc_ok", "q_text", "mine_intro", "st_fixed", "cf_cancel"):
        assert key in g["T"][language], (language, key)
source = Path(__file__).with_name("winlux-report.py").read_text()
assert "check.set_active(False)" in source and "set_default_response(Gtk.ResponseType.CANCEL)" in source
assert "if not confirm_send():" in source and "explain_account" not in source
# #1050 : fenêtre ouverte, le journal du réseau ne part qu'avec la case.
asked = r["Report"](None, language="fr")
asked.interactive = True
assert "Journal du réseau" not in asked.body("Wi-Fi", redacted=True)
asked.attach = ["network"]
assert "Journal du réseau (NetworkManager)" in asked.body("Wi-Fi", redacted=True)

# #1051 : « Se déconnecter » révoque le jeton chez GitHub, sans s'authentifier.
sent_requests = []


class FakeOpener:
    def __init__(self, status=202, error=None):
        self.status, self.error = status, error

    def open(self, req, timeout=20):
        sent_requests.append(req)
        if self.error:
            raise self.error
        status = self.status

        class Answer:
            def __enter__(self):
                return self

            def __exit__(self, *exc):
                return False
        answer = Answer()
        answer.status = status
        return answer


urllib_request = g["urllib"].request
real_opener = urllib_request.build_opener
token = "ghp_" + "a" * 36
try:
    urllib_request.build_opener = lambda *handlers: FakeOpener()
    g["save_account"]("alice-dev", token, "token")
    assert g["disconnect_account"]() == ("alice-dev", "token", "")
    req = sent_requests[-1]
    assert req.full_url.endswith("/credentials/revoke") and req.get_method() == "POST"
    assert req.get_header("Authorization") is None, "révocation : GitHub refuse une requête authentifiée"
    assert g["json"].loads(req.data) == {"credentials": [token]}
    assert g["account"]() == {}
    urllib_request.build_opener = lambda *handlers: FakeOpener(error=g["urllib"].error.URLError("hors ligne"))
    g["save_account"]("alice-dev", "gho_x", "device")
    login, method, error = g["disconnect_account"]()
    assert (login, method) == ("alice-dev", "device") and "hors ligne" in error
    assert g["account"]() == {}, "jeton effacé même si GitHub ne répond pas"
    assert g["REVOKE_PAGES"]["device"].endswith("/settings/apps/authorizations")
    assert g["disconnect_account"]() == ("", "", "")
finally:
    urllib_request.build_opener = real_opener

# #1052 : mes signalements, état demandé à GitHub, file envoyée avec le compte.
assert g["record_sent"]("https://github.com/Univers4craft/winlux/issues/77", "[Bug] a", "account") == 77
assert g["record_sent"]("https://github.com/Univers4craft/winlux/issues/77#issuecomment-1", "[Bug] a", "account") == 77
assert g["record_sent"]("https://example.org/issues/5", "x", "relay") is None
assert [e["number"] for e in g["my_reports"]()] == [77] and g["my_reports"]()[0]["count"] == 2
asked_urls = []


def state_http(method, url, payload=None, token=None, timeout=20):
    asked_urls.append((method, url, token))
    if "creator=alice-dev" in url:
        return [{"number": 90, "title": "[Bug] b", "state": "closed", "state_reason": "not_planned",
                 "created_at": "2026-11-08T10:00:00Z"},
                {"number": 91, "title": "PR", "state": "open", "pull_request": {}}]
    if url.endswith("/issues/77"):
        return {"number": 77, "title": "[Bug] a (corrigé)", "state": "closed", "state_reason": "completed"}
    if "/issues/" in url and method == "POST":
        return {"html_url": "https://github.com/x/y/issues/1"}
    if url.endswith("/repos/Univers4craft/winlux/issues") and method == "POST":
        return {"number": 120, "html_url": "https://github.com/Univers4craft/winlux/issues/120"}
    return {"items": []}


g["http"] = state_http
listed = {e["number"]: e for e in g["refresh_reports"]("tok", "alice-dev")}
assert set(listed) == {77, 90}, listed
assert listed[77]["state"] == "fixed" and listed[77]["title"] == "[Bug] a (corrigé)" and listed[90]["state"] == "closed"
assert all(m == "GET" for m, _, _ in asked_urls)
assert g["report_state"]({"state": "open"}) == "open"
queue = g["QUEUE"]
queue.mkdir(parents=True, exist_ok=True)
for old in queue.glob("*.json"):
    old.unlink()
g["save_json"](queue / "20261108-a.json", {"key": "k1", "signature": "", "kind": "crash", "title": "[Plantage] z.exe",
                                         "labels": ["bug"], "body": "corps", "comment": "re"})
g["clock_ready"] = lambda now=None: True
asked_urls.clear()
assert g["flush"](g["config"](), token="ghu_compte") == 1
assert not list(queue.glob("*.json")) and asked_urls[-1] == ("POST", f"{g['API']}/repos/Univers4craft/winlux/issues", "ghu_compte")
assert 120 in {e["number"] for e in g["my_reports"]()}
assert '"--account-disconnect"' in source and '"--mine"' in source and '"--send-queue"' in source
pages = Path(__file__).with_name("winlux-settings-pages.py").read_text()
assert '"--account-disconnect"' in pages and '"--mine"' in pages and '"--send-queue"' in pages
assert '"--account-forget"]' not in pages, "Se déconnecter doit révoquer le jeton"
g["http"] = real_http
print("winlux-report : tests OK")
