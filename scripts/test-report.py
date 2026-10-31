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
os.environ["XDG_STATE_HOME"] = str(tmp / "state")
(tmp / "journal").mkdir()
(tmp / "run").mkdir()
r = runpy.run_path(str(Path(__file__).with_name("winlux-report.py")), run_name="report_test")

LOG = """[CRASH] ts=100 sig=11 stop=IRQL_NOT_LESS_OR_EQUAL exception=EXCEPTION_ACCESS_VIOLATION code=0x00000000c0000005 exe=Setup.exe module=setup.exe+0x1a2b pid=42
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
# Relais : seuls des rapports winlux-report passent, sans mention ni étiquette inconnue.
relay = runpy.run_path(str(Path(__file__).parent.parent / "integration/report-relay/winlux-report-relay.py"),
                       run_name="relay_test")
title, body, labels, issue = relay["validate"]({"title": report.title(), "body": body + " @damien",
                                               "labels": ["bug", "admin", "plantage"]})
assert labels == ["bug", "plantage", "signalement-utilisateur"] and issue is None
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
print("winlux-report : tests OK")
