#!/usr/bin/python3
"""Explorateur ouvert au repos : il dort jusqu'au prochain message ou tic (#3170).

À lancer dans un affichage X dédié (xvfb-run), après la compilation du
runtime et de gui_demo.exe. L'Explorateur s'ouvre sur un dossier jetable ; on
compte ses changements de contexte (tous ses fils) pendant 5 s sans toucher à
rien, puis on le ferme par son pid.

Attendu : moins de 20 réveils par seconde. Avant, la boucle de gui_demo
dormait par Sleep(10) : ~100 réveils par seconde quoi qu'il arrive.

    test-explorateur-repos-xvfb.py [--runtime target/release/winlux]
                                   [--testdir target/x86_64-pc-windows-msvc/release]
"""
import argparse
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

project = Path(__file__).resolve().parent.parent


def switches(pid):
    total = 0
    for task in Path(f"/proc/{pid}/task").iterdir():
        for line in (task / "status").read_text().splitlines():
            if line.startswith(("voluntary_ctxt_switches", "nonvoluntary_ctxt_switches")):
                total += int(line.split()[1])
    return total


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
        home, run, folder = temp / "home", temp / "run", temp / "dossier"
        for d in (home / ".local/share/winlux", run, folder):
            d.mkdir(parents=True)
        (home / ".local/share/winlux/personnalisation.conf").write_text("oobe=done\n")
        (folder / "note.txt").write_text("x")
        env = dict(os.environ, HOME=str(home), WINLUX_DRIVE_C=str(home), XDG_CONFIG_HOME=str(home / ".config"),
                   XDG_DATA_HOME=str(home / ".local/share"), WINLUX_STORE_RUN=str(run), WINLUX_LANG="fr",
                   WINLUX_AUDIO="null", WINLUX_NO_SPLASH="1", WINLUX_NO_LOGIN="1", WINLUX_NO_OOBE="1",
                   WINLUX_NATIVE_SHELL="1", WINLUX_SHELL_APP="explorer",
                   WINLUX_SHELL_PATH="Z:" + str(folder).replace("/", "\\"))
        proc = subprocess.Popen([args.runtime, str(demo)], env=env, stdout=subprocess.DEVNULL,
                                stderr=subprocess.DEVNULL)
        try:
            time.sleep(4.0)
            if proc.poll() is not None:
                print(f"ÉCHEC Explorateur fermé avant la mesure (code {proc.returncode})")
                return 1
            a = switches(proc.pid)
            time.sleep(5.0)
            b = switches(proc.pid)
        finally:
            os.kill(proc.pid, signal.SIGKILL)
            proc.wait()
    rate = (b - a) / 5.0
    ok = rate < 20
    print(("ok   " if ok else "ÉCHEC ") + f"Explorateur au repos : {rate:.1f} réveils/s (attendu < 20, #3170)")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
