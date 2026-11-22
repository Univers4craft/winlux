#!/usr/bin/python3
"""#2852 : dans la boîte « Enregistrer sous » (l'Explorateur en mode choix),
le nom proposé arrive sélectionné, Ctrl+A le sélectionne de nouveau, et le nom
tapé le remplace au lieu de s'ajouter à la fin (Notepad++ proposait
« nouveau 1.txt », le banc obtenait « nouveau 1.txtessai.txt »).

Comportement attendu, d'après la documentation publique : « When the edit
control receives the input focus, it automatically selects the complete text
for editing. » (Microsoft Learn, Dialog Box Programming Considerations,
section Dialog Box Edit Controls).

À lancer sous xvfb-run, après la compilation du runtime et de gui_demo.exe ;
les touches sont envoyées par xdotool à l'écran virtuel seulement.

    test-pick-selection.py [--runtime target/release/winlux]
                           [--testdir target/x86_64-pc-windows-msvc/release]
"""
import argparse
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

project = Path(__file__).resolve().parent.parent
PROPOSED = "nouveau 1.txt"


def xdotool(*args):
    return subprocess.run(["xdotool", *args], capture_output=True, text=True, timeout=10)


def window_of(pid, seconds=30):
    """Fenêtre visible du processus (le runtime peut lancer un fils)."""
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        out = xdotool("search", "--onlyvisible", "--name", ".").stdout.split()
        for wid in out:
            geo = xdotool("getwindowgeometry", "--shell", wid).stdout
            values = dict(line.split("=", 1) for line in geo.splitlines() if "=" in line)
            if int(values.get("WIDTH", "0")) > 300 and int(values.get("HEIGHT", "0")) > 200:
                return wid, values
        time.sleep(0.5)
    return None, None


def run_case(runtime, demo, work, label, keys_before):
    """Ouvre la boîte avec le nom proposé, joue `keys_before`, tape
    essai.txt puis Entrée, et rend le chemin choisi (ou None)."""
    folder = work / label
    folder.mkdir()
    result = work / f"choix-{label}.txt"
    result.unlink(missing_ok=True)
    env = {k: v for k, v in os.environ.items() if not k.startswith("WINLUX_")}
    env.update({"WINLUX_NATIVE_SHELL": "1", "WINLUX_SHELL_APP": "explorer",
                "WINLUX_SHELL_PATH": "Z:" + str(folder).replace("/", "\\"),
                "WINLUX_PICK": "save", "WINLUX_PICK_RESULT": str(result),
                "WINLUX_PICK_NAME": PROPOSED, "WINLUX_LANG": "fr", "HOME": str(work)})
    process = subprocess.Popen([runtime, demo], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               start_new_session=True)
    try:
        wid, geo = window_of(process.pid)
        if wid is None:
            return None, "fenêtre de la boîte introuvable"
        # Sans gestionnaire de fenêtres, X donne le clavier à la fenêtre sous
        # le pointeur : le pointeur va au milieu de la liste, sans cliquer.
        x = int(geo["X"]) + int(geo["WIDTH"]) // 2
        y = int(geo["Y"]) + int(geo["HEIGHT"]) // 3
        xdotool("mousemove", str(x), str(y))
        xdotool("windowfocus", wid)
        time.sleep(2)
        for key in keys_before:
            xdotool("key", "--clearmodifiers", key)
            time.sleep(0.4)
        xdotool("type", "--delay", "80", "essai.txt")
        time.sleep(0.5)
        xdotool("key", "Return")
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            return None, "la boîte ne s'est pas fermée après Entrée"
        lines = result.read_bytes().decode("latin-1").splitlines() if result.exists() else []
        return (lines[1] if len(lines) > 1 else None), None
    finally:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", default=str(project / "target/release/winlux"))
    parser.add_argument("--testdir", default=str(project / "target/x86_64-pc-windows-msvc/release"))
    args = parser.parse_args()
    demo = str(Path(args.testdir) / "gui_demo.exe")
    if not os.environ.get("DISPLAY"):
        print("test-pick-selection : lancer sous xvfb-run", file=sys.stderr)
        return 2
    if not shutil.which("xdotool"):
        print("test-pick-selection : xdotool manquant", file=sys.stderr)
        return 2
    failures = []
    with tempfile.TemporaryDirectory(prefix="winlux-pick-selection.") as tmp:
        work = Path(tmp)
        cases = [
            # À l'ouverture, le nom proposé est sélectionné : la frappe le remplace.
            ("ouverture", [], "essai.txt"),
            # Fin ôte la sélection, Ctrl+A la remet : la frappe remplace encore.
            ("ctrl-a", ["End", "ctrl+a"], "essai.txt"),
            # Témoin : sans sélection, la frappe s'ajoute à la fin.
            ("temoin", ["End"], PROPOSED + "essai.txt"),
        ]
        for label, keys, wanted in cases:
            chosen, error = run_case(args.runtime, demo, work, label, keys)
            name = chosen.rsplit("\\", 1)[-1] if chosen else None
            text = f"{label} : nom choisi « {wanted} »"
            if error is None and name == wanted:
                print("  OK    ", text)
            else:
                print("  ÉCHEC ", text, f"(obtenu {name!r}{', ' + error if error else ''})")
                failures.append(text)
    if failures:
        return 1
    print("OK : le nom proposé est sélectionné et remplacé, Ctrl+A le sélectionne (#2852)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
