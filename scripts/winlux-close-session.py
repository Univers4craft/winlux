#!/usr/bin/python3
"""Request normal window closure and cancel shutdown if a window refuses."""
import subprocess
import sys
import time

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
            print("Arrêt annulé : une application est encore ouverte. Enregistrez vos documents et fermez-la.", file=sys.stderr)
            return 1
        time.sleep(0.5)
    return 0

if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, subprocess.SubprocessError) as error:
        print(f"Impossible de fermer proprement les applications : {error}", file=sys.stderr)
        sys.exit(1)
