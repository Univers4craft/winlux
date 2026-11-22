#!/usr/bin/python3
"""Applications du shell : rien sur la sortie standard (#2896).

À lancer dans un affichage X dédié (xvfb-run), après la compilation du
runtime et de gui_demo.exe. Les applications du shell PE (Boutique,
Explorateur…) tournent dans gui_demo.exe, qui sert aussi de programme de test
et écrit alors ses lignes « OK … » / « ECHEC … ». Avec WINLUX_TEST, la fenêtre
se ferme seule après 300 ms en passant par tous ses contrôles de fin.

Attendu : ouverte comme application (la Boutique) ou comme bureau, elle
n'écrit aucune ligne de test sur stdout ; lancée en démo, elle garde ses
lignes (les tests du runtime les lisent).

    test-shell-sortie-muette-xvfb.py [--runtime target/release/winlux]
                                     [--testdir target/x86_64-pc-windows-msvc/release]
"""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile

project = Path(__file__).resolve().parent.parent
failures = []


def check(condition, label):
    print(("ok   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", default=str(project / "target/release/winlux"))
    parser.add_argument("--testdir", default=str(project / "target/x86_64-pc-windows-msvc/release"))
    args = parser.parse_args()
    demo = Path(args.testdir) / "gui_demo.exe"
    if not Path(args.runtime).is_file() or not demo.is_file():
        print(f"runtime ou gui_demo.exe absent ({args.runtime}, {demo})")
        return 1
    with tempfile.TemporaryDirectory() as temp:
        temp = Path(temp)
        home, run = temp / "home", temp / "run"
        for d in (home / ".local/share/winlux", run):
            d.mkdir(parents=True)
        (home / ".local/share/winlux/personnalisation.conf").write_text("oobe=done\n")
        base = dict(os.environ, HOME=str(home), WINLUX_DRIVE_C=str(home), XDG_CONFIG_HOME=str(home / ".config"),
                    XDG_DATA_HOME=str(home / ".local/share"), WINLUX_STORE_RUN=str(run), WINLUX_LANG="fr",
                    WINLUX_AUDIO="null", WINLUX_NO_SPLASH="1", WINLUX_NO_LOGIN="1", WINLUX_NO_OOBE="1",
                    WINLUX_TEST="1")

        def lines(**extra):
            r = subprocess.run([args.runtime, str(demo)], env={**base, **extra}, capture_output=True,
                               text=True, errors="replace", timeout=120)
            return r.returncode, [l for l in r.stdout.splitlines() if l.strip().startswith(("OK ", "ECHEC "))]

        code, out = lines(WINLUX_NATIVE_SHELL="1", WINLUX_SHELL_APP="store")
        check(code == 0 and not out, f"Boutique : aucune ligne de test sur stdout (code {code}, {out[:4]})")
        code, out = lines(WINLUX_DESKTOP_DEMO="1")
        check(not out, f"bureau : aucune ligne de test sur stdout ({out[:4]})")
        code, out = lines()
        check(any("RegisterClassExA" in l for l in out) and any("WM_DESTROY" in l for l in out),
              f"démo : lignes de test gardées ({out[:4]})")
    if failures:
        print(f"\n{len(failures)} contrôle(s) en échec")
        return 1
    print("OK : les applications du shell n'écrivent pas de lignes de test sur stdout")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
