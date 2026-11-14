#!/usr/bin/python3
"""#2400 : la boîte Ouvrir / Enregistrer / choix de dossier (l'Explorateur en
mode choix) affiche ce que le programme demande : son titre (lpstrTitle,
IFileDialog::SetTitle) ou la consigne lpszTitle de SHBrowseForFolder, et le
libellé par défaut en quatre langues quand il ne fournit rien.

À lancer sous xvfb-run, après la compilation du runtime et de gui_demo.exe.
Le shell note chaque texte qu'il dessine (WINLUX_CONTRAST_AUDIT avec le seuil
2100, voir gfx::audit_init) ; on y cherche la consigne.

    test-pick-consigne.py [--runtime target/release/winlux]
                          [--testdir target/x86_64-pc-windows-msvc/release]
"""
import argparse
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile

project = Path(__file__).resolve().parent.parent


def drawn(runtime, demo, work, mode, title, lang, seconds=6):
    audit = work / f"audit-{mode}-{lang}.txt"
    audit.unlink(missing_ok=True)
    env = {k: v for k, v in os.environ.items() if not k.startswith("WINLUX_")}
    env.update({"WINLUX_NATIVE_SHELL": "1", "WINLUX_SHELL_APP": "explorer", "WINLUX_SHELL_PATH": "Z:\\tmp",
                "WINLUX_PICK": mode, "WINLUX_PICK_RESULT": str(work / f"choix-{mode}.txt"),
                "WINLUX_CONTRAST_AUDIT": str(audit), "WINLUX_CONTRAST_MINIMUM": "2100",
                "WINLUX_LANG": lang, "HOME": str(work)})
    if title is not None:
        env["WINLUX_PICK_TITLE"] = title
    process = subprocess.Popen([runtime, demo], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               start_new_session=True)
    try:
        process.wait(timeout=seconds)
    except subprocess.TimeoutExpired:
        pass
    finally:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
    return audit.read_bytes().decode("latin-1") if audit.exists() else ""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", default=str(project / "target/release/winlux"))
    parser.add_argument("--testdir", default=str(project / "target/x86_64-pc-windows-msvc/release"))
    args = parser.parse_args()
    demo = str(Path(args.testdir) / "gui_demo.exe")
    if not os.environ.get("DISPLAY"):
        print("test-pick-consigne : lancer sous xvfb-run", file=sys.stderr)
        return 2
    failures = []
    with tempfile.TemporaryDirectory(prefix="winlux-pick-consigne.") as tmp:
        work = Path(tmp)
        consigne = "Choisissez le dossier de destination des fichiers Office"
        cases = [("folder", consigne, "fr", consigne),
                 ("open", "Importer une image", "fr", "Importer une image"),
                 ("save", None, "fr", "Enregistrer sous"),
                 ("folder", None, "en", "Select Folder")]
        for mode, title, lang, wanted in cases:
            text = drawn(args.runtime, demo, work, mode, title, lang)
            # Le libellé du bouton « Select Folder » existe aussi : la consigne
            # par défaut doit apparaître deux fois (bouton + ligne de consigne).
            count = text.count(wanted)
            needed = 2 if title is None and mode == "folder" else 1
            label = f"{mode}/{lang} : « {wanted} » dessiné dans la boîte"
            if count >= needed:
                print("  OK    ", label)
            else:
                print("  ÉCHEC ", label, f"({count} fois, {len(text.splitlines())} textes notés)")
                failures.append(label)
    if failures:
        return 1
    print("OK : la boîte de fichiers affiche la consigne du programme (#2400)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
