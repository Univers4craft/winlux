#!/usr/bin/python3
"""Report measured download and SHA-256 progress to the settings window."""
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import time

def report(phase, done, total, item):
    destination = Path(os.environ.get("WINLUX_PROGRESS_FILE", "/run/winlux/update-progress"))
    destination.parent.mkdir(parents=True, exist_ok=True)
    percent = min(100, done * 100 // total) if total else -1
    temporary = destination.with_name(destination.name + ".new." + str(os.getpid()))
    temporary.write_text(f"phase={phase}\npercent={percent}\ndone={done}\ntotal={total}\nunit=bytes\nitem={item}\n")
    temporary.replace(destination)

def main():
    mode, source, total, *rest = sys.argv[1:]
    total = int(total or 0)
    if mode == "hash":
        total = Path(source).stat().st_size
        digest = hashlib.sha256(); done = 0; last = 0.0
        with open(source, "rb") as stream:
            while chunk := stream.read(1024 * 1024):
                digest.update(chunk); done += len(chunk)
                if time.monotonic() - last >= .25:
                    report("verifying", done, total, "Vérification SHA-256")
                    last = time.monotonic()
        report("verifying", done, total, "Vérification SHA-256")
        print(digest.hexdigest())
    elif mode == "download":
        destination = Path(rest[0])
        if source.startswith("file://"):
            file = Path(source[7:]); total = total or file.stat().st_size
            done = destination.stat().st_size if destination.exists() else 0
            if done > total: destination.unlink(); done = 0
            with file.open("rb") as incoming, destination.open("ab") as outgoing:
                incoming.seek(done)
                while chunk := incoming.read(1024 * 1024):
                    outgoing.write(chunk); done += len(chunk)
                    report("downloading", done, total, "Téléchargement")
        else:
            if not source.startswith(("https://github.com/", "https://objects.githubusercontent.com/")):
                raise ValueError("Source de téléchargement refusée")
            if total and destination.exists() and destination.stat().st_size > total:
                destination.unlink()
            if not (total and destination.exists() and destination.stat().st_size == total):
                for attempt in range(2):
                    resumed = destination.exists() and destination.stat().st_size > 0
                    command = ["curl", "--fail", "--silent", "--show-error", "--location",
                        "--proto", "=https", "--proto-redir", "=https", "--tlsv1.2", "--connect-timeout", "15",
                        "--max-time", "900", "--output", str(destination)]
                    if resumed: command += ["--continue-at", "-"]
                    child = subprocess.Popen(command + [source])
                    try:
                        while child.poll() is None:
                            report("downloading", destination.stat().st_size if destination.exists() else 0, total, "Téléchargement")
                            time.sleep(.25)
                    finally:
                        if child.poll() is None:
                            child.terminate(); child.wait()
                    if child.returncode == 0: break
                    if resumed and child.returncode in (33, 36) and attempt == 0:
                        destination.unlink(missing_ok=True)
                        continue
                    raise subprocess.CalledProcessError(child.returncode, "curl")
        report("downloading", destination.stat().st_size, total, "Téléchargement")
    else: raise ValueError("Mode inconnu")

if __name__ == "__main__":
    try: main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr); sys.exit(1)
