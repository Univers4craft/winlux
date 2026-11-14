#!/usr/bin/env python3
"""#2129 : la Boutique fait accepter le contrat de licence d'un installateur
Windows avant de l'installer sans interface.

Un installateur lancé avec une option silencieuse (/S, /VERYSILENT, /quiet,
msiexec /qn) ne montre pas son contrat. La Boutique ouvre donc d'abord la
page « Contrat de licence » de l'assistant (winlux-setup-wizard
--contrat-seulement, « Je n'accepte pas » coché au départ) : refus, page
fermée ou impossible à ouvrir = rien n'est lancé, et la Boutique dit que le
contrat n'a pas été accepté, dans les quatre langues. Un installateur lancé
avec son interface (« Installer un fichier » sur un .exe) est laissé au
runtime, qui présente lui-même le contrat : la Boutique ne le demande pas
deux fois, et un refus (1602, 66 sur 8 bits) est dit de la même façon.
La page elle-même est testée sous Xvfb par test-licence-installateurs.sh."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

project = Path(__file__).resolve().parent.parent
store = project / "scripts/winlux-store.py"
failures = []
REFUSED = {"fr": "Contrat de licence non accepté : rien n'a été installé.",
           "en": "License agreement not accepted: nothing was installed.",
           "es": "Contrato de licencia no aceptado: no se ha instalado nada.",
           "de": "Lizenzvertrag nicht akzeptiert: Es wurde nichts installiert."}


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
    home, bindir, nowizard, run = w / "home", w / "bin", w / "sans-assistant", w / "run"
    for d in (home, bindir, nowizard, run):
        d.mkdir()
    log = w / "log"
    installer = w / "setup.exe"
    installer.write_bytes(b"MZ installateur NSIS")
    digest = hashlib.sha256(installer.read_bytes()).hexdigest()
    catalog = w / "windows.json"
    catalog.write_text(json.dumps({"apps": [
        {"id": "editeur", "name": "Éditeur", "version": "8.6", "publisher": "Éditeur libre",
         "installer": {"url": installer.as_uri(), "sha256": digest, "args": ["/S"]}}]}))
    conf = home / ".config/winlux/store-sources.d"
    conf.mkdir(parents=True)
    (conf / "local.json").write_text(json.dumps({"id": "local", "name": "Catalogue local", "file": str(catalog)}))
    # Runtime simulé : note chaque appel ; RUNTIME_CODE simule un refus devant
    # la page que le runtime ouvre lui-même (1602 vu sur 8 bits).
    mock(bindir, "winlux", f'printf "winlux %s\\n" "$*" >> "{log}"\nexit ${{RUNTIME_CODE:-0}}\n')
    # Page du contrat : note l'appel et le runtime transmis, répond WIZARD_CODE.
    mock(bindir, "winlux-setup-wizard",
         f'printf "assistant %s | %s\\n" "$*" "$WINLUX_BIN" >> "{log}"\nexit ${{WIZARD_CODE:-0}}\n')
    mock(bindir, "confirm", "exit 0\n")
    for tool in ("winlux", "confirm"):
        (nowizard / tool).symlink_to(bindir / tool)
    env = {"PATH": f"{bindir}:/usr/bin:/bin", "HOME": str(home), "WINLUX_DRIVE_C": str(home),
           "XDG_DATA_HOME": str(home / ".local/share"), "XDG_CACHE_HOME": str(home / ".cache"),
           "XDG_CONFIG_HOME": str(home / ".config"), "WINLUX_STORE_RUN": str(run),
           "WINLUX_STORE_OFFLINE": "1", "WINLUX_STORE_CONFIRM": str(bindir / "confirm"),
           "WINLUX_CONTROL_ROOT": str(w / "root"), "WINLUX_LANG": "fr"}

    def store_run(*args, **extra):
        log.write_text("")
        r = subprocess.run([sys.executable, "-I", "-B", str(store), *args], env={**env, **extra},
                           capture_output=True, text=True, timeout=120)
        shown = run / "store-progress.txt"
        progress = dict(l.split("=", 1) for l in (shown.read_text().splitlines() if shown.is_file() else [])
                        if "=" in l)
        return r.returncode, log.read_text().splitlines(), progress

    store_run("refresh")
    silent = f"winlux {w}/home/.cache/winlux/store/installers/local_editeur.exe /S"

    # Catalogue, installateur NSIS lancé avec /S : contrat refusé.
    code, calls, progress = store_run("install", "windows:local/editeur", WIZARD_CODE="1602")
    ok(any(c.startswith("assistant ") and "local_editeur.exe --contrat-seulement" in c for c in calls),
       f"contrat présenté avant l'installation sans interface ({calls})")
    ok(not any(c.startswith("winlux ") and " /S" in c for c in calls), "contrat refusé : installateur jamais lancé")
    ok(code != 0 and progress.get("state") == "error" and progress.get("message") == REFUSED["fr"],
       f"refus dit à la personne ({code}, {progress.get('message')!r})")
    state = home / ".local/share/winlux/store-state.json"
    ok("local/editeur" not in (json.loads(state.read_text()).get("windows", {}) if state.is_file() else {}),
       "contrat refusé : rien n'est noté comme installé")

    # Même refus dans les quatre langues.
    for lang, text in REFUSED.items():
        _, _, progress = store_run("install", "windows:local/editeur", WIZARD_CODE="1602", WINLUX_LANG=lang)
        ok(progress.get("message") == text, f"{lang} : {progress.get('message')!r}")

    # Contrat accepté (ou paquet sans contrat : l'assistant rend aussi 0).
    code, calls, progress = store_run("install", "windows:local/editeur", WIZARD_CODE="0")
    wizard_at = next((i for i, c in enumerate(calls) if c.startswith("assistant ")), None)
    install_at = next((i for i, c in enumerate(calls) if c == silent), None)
    ok(code == 0 and wizard_at is not None and install_at is not None and wizard_at < install_at,
       f"contrat accepté : installation sans interface ensuite ({calls})")
    ok(wizard_at is not None and calls[wizard_at].endswith(f"| {bindir}/winlux"),
       "l'assistant lit le contrat avec le runtime de la Boutique")

    # Page fermée sans réponse ou impossible à ouvrir (pas d'affichage) : refus.
    code, calls, _ = store_run("install", "windows:local/editeur", WIZARD_CODE="1")
    ok(code != 0 and not any(c == silent for c in calls), "page fermée sans acceptation : rien n'est lancé")
    code, calls, progress = store_run("install", "windows:local/editeur", PATH=f"{nowizard}:/usr/bin:/bin")
    ok(code != 0 and not any(c.startswith("winlux ") and " /S" in c for c in calls)
       and progress.get("message") == REFUSED["fr"], "assistant introuvable : rien n'est lancé")

    # « Installer un fichier » : .msi installé par msiexec /qn, contrat d'abord.
    (home / "outil.msi").write_bytes(b"msi")
    code, calls, progress = store_run("install-file", str(home / "outil.msi"), WIZARD_CODE="1602")
    ok(any(c.startswith(f"assistant {home}/outil.msi --contrat-seulement") for c in calls)
       and not any("/i" in c for c in calls if c.startswith("winlux ")) and progress.get("message") == REFUSED["fr"],
       f"fichier .msi : contrat refusé, msiexec jamais lancé ({calls})")
    code, calls, _ = store_run("install-file", str(home / "outil.msi"), WIZARD_CODE="0")
    ok(code == 0 and f"winlux --msiexec /i {home}/outil.msi /qn" in calls, "fichier .msi : contrat accepté, installé")

    # « Installer un fichier » .exe : interface de l'installateur, le runtime
    # présente lui-même le contrat ; la Boutique ne le demande pas une seconde fois.
    (home / "jeu.exe").write_bytes(b"MZ")
    code, calls, _ = store_run("install-file", str(home / "jeu.exe"))
    ok(code == 0 and calls == [f"winlux {home}/jeu.exe"], f"fichier .exe avec interface : contrat laissé au runtime ({calls})")
    code, calls, progress = store_run("install-file", str(home / "jeu.exe"), RUNTIME_CODE=str(1602 % 256))
    ok(code != 0 and progress.get("message") == REFUSED["fr"],
       f"refus devant la page du runtime (66) dit comme un refus ({progress.get('message')!r})")

if failures:
    sys.exit(f"{len(failures)} échec(s)")
print("Boutique, contrat de licence des installateurs Windows : OK")
