#!/usr/bin/python3
"""Lisibilité du texte des applications maison (shell PE), dans chaque thème.

À lancer dans un affichage X dédié (xvfb-run), après la compilation du
runtime et de gui_demo.exe. Chaque application intégrée (Explorateur,
Paramètres page par page, Store, Gestionnaire des tâches, Pare-feu…) et le
bureau sont ouverts quelques secondes dans chaque thème : clair, sombre,
plusieurs couleurs d'accentuation (dont les plus claires : jaune, turquoise)
et les quatre thèmes de contraste élevé. Le shell, lancé avec
WINLUX_CONTRAST_AUDIT, compare chaque texte dessiné à l'aplat réellement
peint sous lui (gfx::audit_text) et note ceux sous 4,5:1 (3:1 pour le gros
texte). Le test échoue si un seul texte est noté.

    test-pe-text-contrast.py [--runtime target/release/winlux]
                             [--testdir target/x86_64-pc-windows-msvc/release]
                             [--only explorer,settings] [--seconds 5]
"""
import argparse
import concurrent.futures
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

project = Path(__file__).resolve().parent.parent
APPS = ["explorer", "store", "fonts", "programs", "taskschd", "firewall", "taskmgr", "welcome", "regedit",
        "eventviewer", "msinfo32"]
import re  # noqa: E402
# Toutes les pages de Paramètres (PAGE_COUNT de testexe/src/shell/settings.rs).
SETTINGS_PAGES = range(int(re.search(r"const PAGE_COUNT: usize = (\d+);",
                                     (project / "testexe/src/shell/settings.rs").read_text())[1]))
# (nom, WINLUX_THEME, accent RRGGBB, thème de contraste)
THEMES = [("sombre", "dark", "", 0), ("clair", "light", "", 0),
          ("clair jaune", "light", "FFB900", 0), ("clair turquoise", "light", "00B294", 0),
          ("clair gris", "light", "767676", 0), ("sombre rouge", "dark", "C30052", 0),
          ("sombre violet", "dark", "881798", 0)] + \
         [(f"contraste {n}", "dark", "", n) for n in (1, 2, 3, 4)]


def targets(only):
    found = [("bureau", {"WINLUX_DESKTOP_DEMO": "1"})]
    found += [(app, {"WINLUX_NATIVE_SHELL": "1", "WINLUX_SHELL_APP": app}) for app in APPS]
    found += [(f"settings-{page:02d}", {"WINLUX_NATIVE_SHELL": "1", "WINLUX_SHELL_APP": "settings",
                                         "WINLUX_SETTINGS_PAGE": str(page)}) for page in SETTINGS_PAGES]
    return [t for t in found if not only or any(o in t[0] for o in only)]


def run(runtime, demo, environment, seconds):
    process = subprocess.Popen([runtime, demo], env=environment, stdout=subprocess.DEVNULL,
                               stderr=subprocess.DEVNULL, start_new_session=True)
    try:
        process.wait(timeout=seconds)
    except subprocess.TimeoutExpired:
        pass
    finally:
        try:
            os.killpg(process.pid, signal.SIGTERM)
            process.wait(timeout=5)
        except (ProcessLookupError, subprocess.TimeoutExpired):
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", default=str(project / "target/release/winlux"))
    parser.add_argument("--testdir", default=str(project / "target/x86_64-pc-windows-msvc/release"))
    parser.add_argument("--only", default="")
    parser.add_argument("--themes", default="")
    parser.add_argument("--seconds", type=float, default=float(os.environ.get("WINLUX_CONTRAST_SECONDS", "6")))
    parser.add_argument("--jobs", type=int, default=int(os.environ.get("WINLUX_CONTRAST_JOBS", "4")))
    args = parser.parse_args()
    demo = Path(args.testdir) / "gui_demo.exe"
    if not Path(args.runtime).is_file() or not demo.is_file():
        print(f"runtime ou gui_demo.exe absent ({args.runtime}, {demo})")
        return 1
    only = [o for o in args.only.split(",") if o]
    themes = [t for t in THEMES if not args.themes or any(t[0].startswith(x) for x in args.themes.split(","))]
    problems = {}
    with tempfile.TemporaryDirectory() as temp, concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        jobs = []
        for label, theme, accent, contrast in themes:
            for name, extra in targets(only):
                home = Path(temp) / f"{label.replace(' ', '-')}-{name}"
                (home / ".local/share/winlux").mkdir(parents=True)
                (home / ".local/share/winlux/accessibility.ini").write_text(f"contrast={contrast}\n", encoding="latin-1")
                # Pas d'assistant du premier démarrage : préférences déjà enregistrées.
                (home / ".local/share/winlux/personnalisation.conf").write_text("oobe=done\n")
                report = home / "contraste.txt"
                environment = dict(os.environ, HOME=str(home), WINLUX_DRIVE_C=str(home),
                                   XDG_CONFIG_HOME=str(home / ".config"), XDG_DATA_HOME=str(home / ".local/share"),
                                   WINLUX_LANG="fr", WINLUX_AUDIO="null", WINLUX_NO_SPLASH="1", WINLUX_NO_LOGIN="1",
                                   WINLUX_NO_OOBE="1", WINLUX_THEME=theme, WINLUX_CONTRAST_AUDIT=str(report), **extra)
                if accent:
                    environment["WINLUX_ACCENT"] = accent
                jobs.append((label, name, report, pool.submit(run, args.runtime, str(demo), environment, args.seconds)))
        for label, name, report, job in jobs:
            job.result()
            if report.is_file():
                for line in report.read_text(encoding="cp1252", errors="replace").splitlines():
                    ratio, fg, bg, text = (line.split("\t") + ["", "", ""])[:4]
                    problems.setdefault((name, text, fg, bg, ratio), []).append(label)
    if problems:
        for (name, text, fg, bg, ratio), labels in sorted(problems.items()):
            print(f"{name} · « {text} » : {ratio}:1 ({fg} sur {bg}) — {', '.join(labels)}")
        print(f"\n{len(problems)} texte(s) du shell PE sous le seuil WCAG AA")
        return 1
    print(f"OK : texte du shell PE lisible dans {len(targets(only))} écran(s) × {len(themes)} thème(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
