#!/usr/bin/python3
"""Request normal window closure and cancel shutdown if a window refuses."""
import subprocess
import sys
import time
import os


# ── Langue de l'interface (Paramètres > Langue) : fr, en, es ou de ──────────

def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="):
                        code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}

# Texte français -> (anglais, espagnol, allemand).
_TR = {
    'Arrêt annulé : une application est encore ouverte. Enregistrez vos documents et fermez-la.': ('Shutdown canceled: an app is still open. Save your documents and close it.', 'Apagado cancelado: una aplicación sigue abierta. Guarde sus documentos y ciérrela.', 'Herunterfahren abgebrochen: Eine App ist noch geöffnet. Speichern Sie Ihre Dokumente und schließen Sie sie.'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


def windows():
    output = subprocess.check_output(["wmctrl", "-lx"], text=True, stderr=subprocess.PIPE)
    result = set()
    for line in output.splitlines():
        fields = line.split(None, 4)
        if len(fields) < 3:
            continue
        window_class = fields[2].casefold()
        if any(name in window_class for name in ("xfce4-panel", "xfdesktop", "winlux-start")):
            continue
        result.add(fields[0])
    return result

def main():
    initial = windows()
    for window in initial:
        subprocess.run(["wmctrl", "-ic", window], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    deadline = time.monotonic() + 30
    while initial & windows():
        if time.monotonic() >= deadline:
            print(_t("Arrêt annulé : une application est encore ouverte. Enregistrez vos documents et fermez-la."), file=sys.stderr)
            return 1
        time.sleep(0.5)
    return 0

if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, subprocess.SubprocessError) as error:
        print(f"Impossible de fermer proprement les applications : {error}", file=sys.stderr)
        sys.exit(1)
