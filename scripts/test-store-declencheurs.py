#!/usr/bin/env python3
"""#2186 : pendant les déclencheurs de dpkg (shared-mime-info…), la Boutique
dit qu'elle met le système à jour, sans pourcentage, au lieu d'un 99 % figé
pendant plusieurs minutes. Le clic pendant une opération, expliqué dans le
bandeau du Store, est vérifié à l'écran par test-store-bandeau-xvfb.py."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

project = Path(__file__).resolve().parent.parent
store = project / "scripts/winlux-store.py"
failures = []

def ok(cond, label):
    print(("OK    " if cond else "ÉCHEC ") + label)
    if not cond:
        failures.append(label)

def mock(bindir, name, body):
    p = bindir / name
    p.write_text("#!/bin/sh\n" + body)
    p.chmod(0o755)

with tempfile.TemporaryDirectory() as tmp:
    w = Path(tmp)
    root, home, bindir, run = w / "root", w / "home", w / "bin", w / "run"
    for d in (root, home, bindir, run):
        d.mkdir()
    seen = w / "seen"
    mock(bindir, "dpkg-query", "exit 0\n")
    mock(bindir, "apt", "exit 0\n")
    mock(bindir, "apt-cache", "exit 0\n")
    # apt-get : étapes ordinaires puis déclencheurs ; la maquette relève ce
    # que la Boutique affiche pendant chacune.
    mock(bindir, "apt-get", f"""fd=; for a in "$@"; do case "$a" in APT::Status-Fd=*) fd=${{a#APT::Status-Fd=}};; esac; done
step() {{ eval "echo '$1' >&$fd"; sleep 0.5; cat "{run}/store-progress.txt" >> "{seen}"; echo '--' >> "{seen}"; }}
step 'pmstatus:lynx:80.0:Installed lynx (amd64)'
step 'pmstatus:shared-mime-info:99.0:Processing triggers for shared-mime-info (2.4-4)'
exit 0
""")
    for tool in ("sudo", "pkexec"):
        mock(bindir, tool, '[ "$1" = -n ] && shift\nexec "$@"\n')
    (home / ".cache/winlux/store").mkdir(parents=True)
    env = {"PATH": f"{bindir}:/usr/bin:/bin", "HOME": str(home), "WINLUX_CONTROL_ROOT": str(root),
           "XDG_CACHE_HOME": str(home / ".cache"), "XDG_DATA_HOME": str(home / ".local/share"),
           "WINLUX_STORE_RUN": str(run), "WINLUX_STORE_OFFLINE": "1",
           "WINLUX_STORE_CATALOGS_DEFS": str(w / "aucun.json")}
    expected = {"fr": "Mise à jour du système après l'installation (shared-mime-info)",
                "en": "Updating the system after installation (shared-mime-info)",
                "es": "Actualizando el sistema tras la instalación (shared-mime-info)",
                "de": "System wird nach der Installation aktualisiert (shared-mime-info)"}
    for lang, words in expected.items():
        seen.write_text("")
        (home / ".cache/winlux/store/index.json").write_text(json.dumps(
            [{"source": "apt", "ref": "lynx", "name": "Lynx", "state": "a"}]))
        subprocess.run([sys.executable, "-I", "-B", str(store), "install", "apt:lynx"],
                       env={**env, "WINLUX_LANG": lang}, capture_output=True, text=True, timeout=120)
        steps = [dict(l.split("=", 1) for l in block.strip().splitlines() if "=" in l)
                 for block in seen.read_text().split("--\n") if block.strip()]
        ok(len(steps) == 2 and steps[0].get("percent") == "90", f"{lang} : étape ordinaire avec pourcentage ({steps[:1]})")
        trig = steps[1] if len(steps) > 1 else {}
        ok(words in trig.get("message", "") and trig.get("percent") == "",
           f"{lang} : déclencheurs annoncés sans pourcentage ({trig.get('message')!r}, {trig.get('percent')!r}) (#2186)")

if failures:
    sys.exit(f"{len(failures)} échec(s)")
print("Boutique, fin d'installation expliquée : OK")
