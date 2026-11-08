#!/usr/bin/python3
"""Page Mise à jour OS refaite (#875 à #882), sans root ni réseau.

Vérifie la page (états de la carte, tailles lisibles, dates, notes de
version, historique, notification, quatre langues) et le moteur APT
(suspension, heures d'activité, historique, état clé=valeur), ainsi que les
actions de winlux-control et du relais root.
"""
import ast
import json
import os
from pathlib import Path
import re
import runpy
import subprocess
import tempfile
import time

HERE = Path(__file__).resolve().parent
checks = 0


def ok(condition, label):
    global checks
    if not condition:
        raise SystemExit(f"ÉCHEC : {label}")
    checks += 1
    print(f"  OK     {label}")


def page(lang="fr"):
    os.environ["WINLUX_LANG"] = lang
    return runpy.run_path(str(HERE / "winlux-update-page.py"), run_name=f"update_page_{lang}")


# ── Quatre langues : chaque texte de la page a sa traduction ────────────
source = (HERE / "winlux-update-page.py").read_text(encoding="utf-8")
table = next(ast.literal_eval(node.value) for node in ast.parse(source).body
             if isinstance(node, ast.Assign) and any(getattr(t, "id", "") == "_TR" for t in node.targets))
used = set(re.findall(r'_t\("((?:[^"\\]|\\.)*)"\)', source))
module = page()
used |= set(module["STEPS"]) | set(module["PHASE_NAMES"].values())
used |= {"Installée", "Échec", "Restaurée à la main", "Restaurée automatiquement", "Nouveautés de votre version ({version})",
         "Nouveautés de la version {version}"}
missing = sorted(text for text in used if text not in table)
ok(not missing, f"tous les textes traduits en anglais, espagnol et allemand ({len(used)})" + (f" — manquent : {missing}" if missing else ""))
ok(all(len(row) == 3 and all(value.strip() for value in row) for row in table.values()), "aucune traduction vide")
for key, row in table.items():
    for value in row:
        ok(set(re.findall(r"{\w+}", value)) == set(re.findall(r"{\w+}", key)), f"mêmes champs dans « {key[:40]} »") if "{" in key else None

# ── Tailles, dates, notes ───────────────────────────────────────────────
ok(module["human_size"]("485490688") == "463 Mo", "463 Mo au lieu de 485490688 octets (#875)")
ok(module["human_size"](1610612736) == "1,5 Go", "1,5 Go")
ok(page("en")["human_size"]("485490688") == "463 MB", "463 MB en anglais")
module = page()
now = time.mktime((2026, 11, 8, 10, 0, 0, 0, 0, -1))
ok(module["when"](now - 3600, now) == "aujourd’hui à 09:00", "aujourd’hui à 09:00")
ok(module["when"](now - 86400, now) == "hier à 10:00", "hier")
ok(module["when"](now - 5 * 86400, now) == "le 03/11/2026 à 10:00", "date complète")
ok(page("de")["when"](now - 5 * 86400, now) == "am 03.11.2026 um 10:00", "date allemande")
module = page()
notes = "# Winlux v0.2.21\n\n## Programmes\n\n- Premier point corrigé. Détail long\n  sur deux lignes (#12, #34).\n- **Gras** et `code` <b>.\n\nTexte libre.\n"
full = module["notes_markup"](notes)
ok("Winlux v0.2.21" not in full and "<b>Programmes</b>" in full, "titres en gras, titre de version retiré")
ok("Détail long sur deux lignes." in full and "#12" not in full, "puces recollées, numéros d'issues retirés")
ok("<b>Gras</b>" in full and "<tt>code</tt>" in full and "&lt;b&gt;" in full, "gras, code, balises échappées")
short = module["notes_markup"](notes, limit=1)
ok(short == "•  Premier point corrigé.", f"aperçu : première phrase de la première puce ({short!r})")
releases = sorted((HERE.parent / "docs/releases").glob("v0.2.*.md"))
import gi
gi.require_version("Pango", "1.0")
from gi.repository import Pango
for release in releases[-5:]:
    text = release.read_text(encoding="utf-8")
    for markup in (module["notes_markup"](text), module["notes_markup"](text, limit=5)):
        Pango.parse_markup(markup, -1, "\0")
ok(True, "notes des 5 dernières versions : balisage Pango valide")

# ── États de la carte ───────────────────────────────────────────────────
situation = module["situation"]
base = {"installed": "v0.2.20", "last_check": str(int(now - 600))}
kind, title, subtitle, action = situation(base, {}, False, now=now)
ok((kind, action) == ("uptodate", "check") and "Dernière recherche : aujourd’hui à 09:50" in subtitle, "à jour + dernière recherche (#882)")
ok(situation({"installed": "v0.2.20"}, {}, False, now=now)[0] == "never", "jamais recherché")
available = dict(base, available="v0.2.21", download_bytes="485490688")
kind, _, subtitle, action = situation(available, {}, False, now=now)
ok(action == "install" and "463 Mo à télécharger" in subtitle, "disponible : Télécharger et installer")
ok(situation(available, {"phase": "downloading"}, True, now=now)[:2] == ("running", "Installation de Winlux v0.2.21"), "en cours")
ok(situation(available, {"phase": "checking"}, True, now=now)[0] == "checking", "recherche en cours")
ok(situation(dict(available, paused_until=str(int(now + 86400))), {}, False, now=now)[3] == "resume", "suspendu : Reprendre")
ok(situation(dict(available, paused_until=str(int(now - 1))), {}, False, now=now)[3] == "install", "pause échue : plus suspendu")
ok(situation(dict(base, reboot="1", previous="v0.2.19"), {}, False, now=now)[3] == "reboot", "redémarrage nécessaire (#879)")
ok(situation(base, {}, False, failed="downloading", now=now)[:2] == ("error", "La mise à jour a échoué"), "échec")
ok(module["progress_text"]({"phase": "downloading", "unit": "bytes", "done": "180000000", "total": "485490688"})
   == "Téléchargement · 171,7 Mo sur 463 Mo", "progression lisible")
ok(module["legacy_state"]("Version installée : v0.2.19\nVersion disponible : v0.2.20\nTaille à télécharger : 10 octets\n")
   == {"installed": "v0.2.19", "available": "v0.2.20", "download_bytes": "10"}, "ancien moteur : rapport relu")

# ── Historique et notification ──────────────────────────────────────────
with tempfile.TemporaryDirectory() as folder:
    folder = Path(folder)
    (folder / "update-history").write_text('{"date": 1, "version": "v0.2.19", "result": "installed"}\npas du json\n'
                                           '{"date": 2, "version": "v0.2.20", "result": "failed"}\n')
    entries = module["load_history"](folder)
    ok([entry["version"] for entry in entries] == ["v0.2.20", "v0.2.19"], "historique : plus récent d'abord, ligne abîmée ignorée")
    (folder / "notes").mkdir()
    (folder / "notes/v0.2.21.md").write_text("- point\n")
    ok(module["load_notes"]("v0.2.21", folder) == "- point" and module["load_notes"]("../x", folder) == "", "notes : nom de version vérifié")
for lang in ("fr", "en", "es", "de"):
    translated = page(lang)
    for kind in ("available", "reboot"):
        message = translated["notification"]({"kind": kind, "version": "v0.2.21"})
        ok(message and "v0.2.21" in message[1], f"notification « {kind} » en {lang} (#881)")
module = page()
ok(module["notification"]({"kind": "available", "version": "../x"}) is None, "notification : version vérifiée")
with tempfile.TemporaryDirectory() as folder:
    run = Path(folder)
    (run / "update-notice").write_text("kind=available\nversion=v0.2.21\n")
    module["RUN"] = run
    module["notify"].__globals__["RUN"] = run
    seen = run / "seen"
    seen.write_text("available:v0.2.21\n")
    ok(module["notify"](seen) == 0 and seen.read_text() == "available:v0.2.21\n", "une seule notification par version")

# ── Moteur APT : pause, heures d'activité, historique, état ─────────────
engine = runpy.run_path(str(HERE / "winlux-update-apt.py"), run_name="update_engine_test")
glob = engine["main"].__globals__
with tempfile.TemporaryDirectory() as folder:
    folder = Path(folder)
    for name, value in (("STATE", folder), ("RUN", folder / "run"), ("CONFIG", folder / "update.conf"),
                        ("HISTORY", folder / "update-history"), ("LAST_CHECK", folder / "update-last-check"),
                        ("NOTES", folder / "notes"), ("NOTICE", folder / "run/update-notice"), ("CACHE", folder / "cache"),
                        ("TRIAL", folder / "trial"), ("BLOCKED", folder / "blocked"), ("ROLLED_BACK", folder / "rolled"),
                        ("RESTORE_POINT", folder / "point")):
        glob[name] = value
    (folder / "run").mkdir()
    glob["CONFIG"].write_text("auto=enabled\nmanifest=/x\n")
    glob["set_config"]("pause_until", 4102444800)
    ok("auto=enabled" in glob["CONFIG"].read_text() and "manifest=/x" in glob["CONFIG"].read_text(), "réglages gardés")
    ok(glob["paused_until"](now) == 4102444800, "suspension lue")
    glob["set_config"]("pause_until", None)
    ok(glob["paused_until"](now) == 0 and "pause_until" not in glob["CONFIG"].read_text(), "reprise")
    ok(glob["active_hours"]() == (8, 18), "heures d'activité par défaut 8 h – 18 h")
    glob["set_config"]("active_hours", "22-6")
    ok(glob["active_hours"]() == (22, 6), "heures d'activité réglées")
    ok(glob["in_active_hours"](23) and glob["in_active_hours"](5) and not glob["in_active_hours"](12), "plage qui passe minuit")
    ok(glob["in_active_hours"](9, (8, 18)) and not glob["in_active_hours"](18, (8, 18)), "8 h – 18 h")
    for index in range(55): glob["record"]("installed", f"v0.{index}", "v0.0")
    lines = glob["HISTORY"].read_text().splitlines()
    ok(len(lines) == 50 and json.loads(lines[-1])["version"] == "v0.54", "historique : 50 entrées gardées")
    ok(oct(glob["HISTORY"].stat().st_mode & 0o777) == "0o644", "historique lisible par le compte")
    real_read = glob["read"]
    glob["read"] = lambda path: {"/usr/share/winlux/version": "v0.2.20", "/usr/share/winlux/update-backend": "apt"}.get(str(path), real_read(path))
    glob["package_version"] = lambda _path: "0.2.19"
    (folder / "available-manifest").write_text("format=3\nbackend=apt\nbase=ubuntu-resolute\nversion=v0.2.21\nasset=winlux-desktop-v0.2.21.deb\n"
                                               f"sha256={'a' * 64}\ndownload_bytes=485490688\nunpacked_kib=1\n")
    (folder / "notes").mkdir()
    (folder / "notes/v0.2.21.md").write_text("- x\n")
    import contextlib, io
    output = io.StringIO()
    with contextlib.redirect_stdout(output): glob["state"]()
    values = dict(line.split("=", 1) for line in output.getvalue().splitlines())
    ok(values["available"] == "v0.2.21" and values["notes"] == "v0.2.21" and values["previous"] == "v0.2.19"
       and values["auto"] == "enabled" and values["active_hours"] == "22-6", "--state : état clé=valeur complet")
    # Installation automatique : rien pendant une pause, rien pendant les heures d'activité.
    glob["sys"].argv = ["winlux-update-apt", "--auto-run"]
    glob["set_config"]("pause_until", int(time.time()) + 3600)
    ok(glob["main"]() == 0 and not (folder / "apt-update.lock").exists(), "--auto-run : rien pendant une pause")
    glob["set_config"]("pause_until", None)
    hour = time.localtime().tm_hour
    glob["set_config"]("active_hours", f"{hour}-{(hour + 1) % 24}")
    ok(glob["main"]() == 0 and not (folder / "apt-update.lock").exists(), "--auto-run : rien pendant les heures d'activité")
    for args in (["--pause", "9"], ["--active-hours", "8", "8"], ["--active-hours", "8", "25"]):
        glob["sys"].argv = ["winlux-update-apt"] + args
        try:
            glob["main"](); refused = False
        except ValueError:
            refused = True
        ok(refused, f"arguments refusés : {' '.join(args)}")
    glob["sys"].argv = ["winlux-update-apt", "--pause", "2"]
    ok(glob["main"]() == 0 and glob["paused_until"]() > time.time() + 13 * 86400, "--pause 2 : deux semaines")
    glob["sys"].argv = ["winlux-update-apt", "--active-hours", "9", "17"]
    ok(glob["main"]() == 0 and glob["active_hours"]() == (9, 17), "--active-hours 9 17")

# ── winlux-control, relais root, paquet, session ────────────────────────
control = (HERE / "winlux-control.sh").read_text(encoding="utf-8")
ok("update-pause-1|update-pause-2|update-pause-3|update-pause-4|update-pause-5)" in control and "update-resume)" in control
   and 'winlux-update --active-hours "${hours%-*}" "${hours#*-}"' in control, "winlux-control : pause, reprise, heures")
ok("winlux-update --state" in control and 'test -s "$run/update-state.txt"' in control, "winlux-control publie update-state.txt")
root = runpy.run_path(str(HERE / "winlux-root.py"), run_name="update_root_test")
patterns = [pattern for pattern, _ in root["CONTROL_POLICY"]]
def allowed(action):
    return any(re.fullmatch(pattern, action) for pattern in patterns)
ok(allowed("update-pause-3") and allowed("update-resume") and allowed("update-hours-8-18") and not allowed("update-pause-9"),
   "relais root : actions de la page autorisées, rien d'autre")
ok("update-state.txt" in root["GIVE_BACK"], "état rendu au compte après une action relayée")
ok("'winlux-update-page.py:winlux-update-page'" in (HERE / "stage-desktop-components.sh").read_text(), "page installée dans le paquet")
ok("notes_asset=winlux-release-notes.txt" in (HERE / "build-apt-update.py").read_text(), "notes de version publiées avec le paquet APT (#880)")
ok("winlux-update-page notify" in (HERE / "winlux-user-session.sh").read_text(), "notification lancée par la session (#881)")
print(f"OK : {checks} vérifications de la page Mise à jour OS")
