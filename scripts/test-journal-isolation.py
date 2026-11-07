#!/usr/bin/python3
"""Journal et état Winlux isolés entre comptes (#633).

Avant : /run/winlux/journal en 1777 avec winlux.log et crash.log en 0666,
crash-offer, crash-bsod et report-offered communs à tous les comptes,
brightness-last en 0666, fs.protected_regular à 0. N'importe quel compte
pouvait effacer ou falsifier les plantages d'un autre (envoyés sur GitHub
par winlux-report) et le second compte ne voyait jamais ses propres écrans
bleus. Ce test vérifie, sans root ni VM, que rien ne revient en arrière.
"""
import os
from pathlib import Path
import re
import runpy
import stat
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
checks = 0


def ok(condition, label):
    global checks
    if not condition:
        print(f"ÉCHEC : {label}", file=sys.stderr)
        sys.exit(1)
    checks += 1


def text(path):
    return (ROOT / path).read_text(encoding="utf-8")


# ── Init : plus de journal partagé en écriture, protected_regular ─────────
init = text("scripts/build-live-iso.sh")
ok("chmod 1777 /run/winlux/journal" not in init, "init : /run/winlux/journal n'est plus en 1777")
ok(not re.search(r"chmod 0?666 \"?/run/winlux", init), "init : aucun fichier de /run/winlux en 0666")
ok("chmod 0755 /run/winlux/journal" in init, "init : journal des démons root en 0755")
ok("mkdir -m 0700 \"$journal_dir\"" in init and "journal-$journal_uid" in init,
   "init : un journal 0700 par compte, créé avant les sessions")
ok("protected_regular" in init and "echo 2 >/proc/sys/fs/protected_regular" in init,
   "init : fs.protected_regular=2 (fichier créé à l'avance par un autre compte)")
ok("chmod 1777 /run/winlux\n" in init, "init : /run/winlux reste collant pour les dossiers par compte")

# ── Personne n'écrit plus à un chemin commun ──────────────────────────────
shared = re.compile(r"/run/winlux/(crash-offer|crash-bsod|report-offered)|"
                    r"Z:\\\\run\\\\winlux\\\\(crash-offer|crash-bsod|journal\\\\)|"
                    r"/run/winlux/journal/(crash|winlux)\.log|"
                    r"RUN / \"(crash-offer|report-offered)\"")
for folder, pattern in (("runtime/src", "*.rs"), ("testexe/src", "*.rs"), ("scripts", "winlux-*"),
                        ("integration", "winlux-*")):
    for path in sorted((ROOT / folder).rglob(pattern)):
        if path.is_file():
            body = path.read_text(encoding="utf-8", errors="replace")
            hit = shared.search(body)
            ok(hit is None, f"{path.relative_to(ROOT)} : chemin de journal commun à tous les comptes ({hit and hit.group(0)})")

runtime = text("runtime/src/journal.rs")
ok("journal-{euid}" in runtime and "0o700" in runtime, "runtime : /run/winlux/journal-<uid> en 0700")
ok("meta.uid() == euid" in runtime, "runtime : refuse un dossier qui n'est pas au compte")
crash = text("runtime/src/crash.rs")
ok("O_NOFOLLOW" in crash and "signal_paths()" in crash, "runtime : crash.log sans suivre de lien, chemins préparés")

# ── Changement d'utilisateur : journaux du bureau hors de /run/winlux ────
sessions = text("scripts/winlux-sessions.sh")
ok('log="$run/session-' not in sessions and '-logfile "$run/Xorg' not in sessions,
   "winlux-sessions : session-<compte>.log et Xorg.<n>.log plus dans /run/winlux (collant)")
ok('own_dir "$run/session-logs"' in sessions and '"$out.tmp.$$"' not in sessions,
   "winlux-sessions : dossier de journaux à root, liste par mktemp")
with tempfile.TemporaryDirectory() as tmp:
    run = Path(tmp) / "run/winlux"
    run.mkdir(parents=True)
    (run / "session-logs").symlink_to(tmp)
    script = ROOT / "scripts/winlux-sessions.sh"
    body = script.read_text(encoding="utf-8")
    helper = body[body.index("own_dir() {"):body.index("do_list() {")]
    out = subprocess.run(["sh", "-c", "fail() { echo \"$*\" >&2; exit 1; }\n" + helper + 'own_dir "$1"', "x",
                          str(run / "session-logs")], capture_output=True, text=True)
    ok(out.returncode == 0 and not (run / "session-logs").is_symlink() and (run / "session-logs").is_dir(),
       f"winlux-sessions : un lien posé à la place du dossier est remplacé ({out.stderr})")

# ── Lecteurs : même dossier par défaut que le runtime ────────────────────
env = {k: v for k, v in os.environ.items() if k != "WINLUX_JOURNAL_DIR"}
expected = "/run/winlux/journal" if os.geteuid() == 0 else f"/run/winlux/journal-{os.geteuid()}"
for script in ("scripts/winlux-report.py", "scripts/winlux-assistant.py"):
    line = next(l for l in text(script).splitlines() if l.startswith("JOURNAL = "))
    saved = os.environ.pop("WINLUX_JOURNAL_DIR", None)
    value = eval(line.split("=", 1)[1], {"Path": Path, "os": os})  # ligne du dépôt
    if saved is not None:
        os.environ["WINLUX_JOURNAL_DIR"] = saved
    ok(str(value) == expected, f"{script} : journal par défaut {value} (attendu {expected})")
out = subprocess.run(["sh", str(ROOT / "scripts/winlux-journal.sh"), "status"], env=env,
                     capture_output=True, text=True)
ok(out.returncode == 0, f"winlux-journal status : {out.stderr}")
with tempfile.TemporaryDirectory() as tmp:
    journal = Path(tmp)
    (journal / "winlux.log").write_text("x" * 1048577)
    out = subprocess.run(["sh", str(ROOT / "scripts/winlux-journal.sh"), "rotate"],
                         env={**env, "WINLUX_JOURNAL_DIR": tmp}, capture_output=True, text=True)
    mode = stat.S_IMODE((journal / "winlux.log").stat().st_mode)
    ok(out.returncode == 0 and mode & 0o077 == 0, f"winlux-journal rotate : nouveau winlux.log privé ({mode:o})")

# ── Luminosité : brightness-last n'est plus écrivable par tous ────────────
with tempfile.TemporaryDirectory() as tmp:
    os.environ["WINLUX_BRIGHTNESS_RUN"] = tmp
    module = runpy.run_path(str(ROOT / "scripts/winlux-brightness.py"), run_name="winlux_brightness")
    module["note_last"](Path("/sys/class/backlight/intel_backlight"), 1000, 40)
    last = Path(tmp) / "brightness-last"
    ok(last.read_text().split() == ["intel_backlight", "400", "40"], "luminosité : brightness-last écrit")
    mode = stat.S_IMODE(last.stat().st_mode)
    ok(mode & 0o022 == 0, f"luminosité : brightness-last pas écrivable par les autres ({mode:o})")
    ok(sorted(p.name for p in Path(tmp).iterdir()) == ["brightness-last"], "luminosité : pas de fichier temporaire laissé")

print(f"Journal isolé entre comptes : {checks} vérifications OK")
