#!/usr/bin/python3
"""winlux-power : l'arrêt signale vraiment l'init, et seulement quand il le doit.

Sans affichage : l'étape finale (signal à l'init, mise à jour en cours), le
relais pour un second compte (socket, compte connecté ou refusé) et la liste
des fenêtres à fermer avant l'arrêt.
"""
import os
from pathlib import Path
import runpy
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from unittest.mock import patch

script = Path(__file__).with_name("winlux-power.py")

with tempfile.TemporaryDirectory() as temp:
    temp = Path(temp)
    marker = temp / "init.txt"
    # Faux PID 1 : le même contrat que l'init de Winlux (USR2 arrêt, TERM redémarrage).
    init = subprocess.Popen(["sh", "-c", f'trap "echo poweroff > {marker}; exit" USR2; '
                             f'trap "echo reboot > {marker}; exit" TERM; while :; do sleep 0.1; done'])
    time.sleep(0.3)
    os.environ.update(WINLUX_CONTROL_ROOT=str(temp), WINLUX_POWER_INIT_PID=str(init.pid),
                      WINLUX_POWER_SOCKET=str(temp / "power.sock"), WINLUX_LANG="fr")
    module = runpy.run_path(str(script), run_name="power_test")
    namespace = module["execute"].__globals__

    # Une installation en cours bloque l'arrêt, sans toucher à l'init.
    with patch.dict(namespace, {"updates_busy": lambda: True}):
        assert module["execute"]("shutdown") == module["UPDATE_BUSY"]
    assert init.poll() is None and not marker.exists()

    # Arrêt : SIGUSR2 à l'init (pas « poweroff », qui vise systemd).
    with patch.dict(namespace, {"updates_busy": lambda: False}):
        assert module["execute"]("shutdown") is None
    init.wait(5)
    assert marker.read_text().strip() == "poweroff"

    # Redémarrage : SIGTERM.
    marker.unlink()
    init = subprocess.Popen(["sh", "-c", f'trap "echo poweroff > {marker}; exit" USR2; '
                             f'trap "echo reboot > {marker}; exit" TERM; while :; do sleep 0.1; done'])
    time.sleep(0.3)
    namespace["INIT_PID"] = init.pid
    with patch.dict(namespace, {"updates_busy": lambda: False}):
        assert module["execute"]("restart") is None
    init.wait(5)
    assert marker.read_text().strip() == "reboot"

    # Actions inconnues refusées.
    for bad in ("", "suspend", "poweroff; rm -rf /"):
        try:
            module["execute"](bad)
        except ValueError:
            pass
        else:
            raise AssertionError(f"action acceptée : {bad!r}")

    # Relais pour un second compte : seul un compte avec un bureau ouvert passe.
    requests = []
    with patch.dict(namespace, {"execute": lambda action: requests.append(action)}):
        server = threading.Thread(target=module["serve"], daemon=True)
        server.start()
        for _ in range(50):
            if (temp / "power.sock").exists():
                break
            time.sleep(0.05)
        uid = os.geteuid()
        if uid != 0:
            assert "Accès refusé" in module["request_from_service"]("poweroff")
            assert requests == []
            import pwd
            (temp / "run/winlux/sessions").mkdir(parents=True)
            (temp / "run/winlux/sessions" / pwd.getpwuid(uid).pw_name).write_text("display=:1\n")
        assert module["request_from_service"]("reboot") is None
        assert requests == ["reboot"]
        assert module["request_from_service"]("format") == "Action inconnue"
        # Un second « serve » voit le premier et s'arrête aussitôt.
        assert module["serve"]() == 0

    # Fenêtres à fermer : ni le bureau, ni la barre des tâches, ni les fenêtres collantes.
    listing = ("0x01 -1 10 xfdesktop.Xfdesktop host Bureau\n"
               "0x02 0 11 xfce4-panel.Xfce4-panel host xfce4-panel\n"
               "0x03 0 12 mousepad.Mousepad host Rapport.txt - Bloc-notes\n"
               "0x04 0 13 winlux.Notepad.exe host Sans titre - Bloc-notes\n")
    with patch.object(namespace["subprocess"], "check_output", return_value=listing):
        windows = module["open_windows"]()
    assert sorted(windows) == ["0x03", "0x04"], windows
    assert windows["0x03"][1] == "Rapport.txt - Bloc-notes"

print("OK: arrêt et redémarrage signalent l'init, mise à jour bloquante, relais limité aux comptes connectés")
