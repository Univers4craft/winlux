#!/usr/bin/python3
"""Redémarrage depuis l'installateur et écran d'arrêt qui plante.

Défaut v0.2.14 (session live) : « Redémarrer maintenant » fermait les
applications mais ne redémarrait pas, car tout reposait sur l'écran GTK de
winlux-power ; s'il mourait (« Bus error » déjà vu en v0.2.13), personne ne
signalait l'init. On vérifie, sans affichage :
  1. « now reboot » dont l'écran plante redémarre quand même (SIGTERM à l'init) ;
  2. « ask reboot » dont l'écran plante avant la confirmation ne fait rien ;
  3. l'installateur passe par l'étape finale sans écran (« execute reboot »)
     et ne garde l'écran que si l'init refuse.
"""
import ast
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

here = Path(__file__).resolve().parent
power = here / "winlux-power.py"


def fake_init(marker):
    # Même contrat que l'init de Winlux : USR2 = arrêt, TERM = redémarrage.
    process = subprocess.Popen(["sh", "-c", f'trap "echo poweroff > {marker}; exit" USR2; '
                                f'trap "echo reboot > {marker}; exit" TERM; while :; do sleep 0.1; done'])
    time.sleep(0.3)
    return process


with tempfile.TemporaryDirectory() as temp:
    temp = Path(temp)
    tools = temp / "bin"
    tools.mkdir()
    for name in ("sh", "sleep"):
        found = subprocess.run(["sh", "-c", f"command -v {name}"], capture_output=True, text=True).stdout.strip()
        if found:
            (tools / name).symlink_to(found)
    base = dict(os.environ, WINLUX_CONTROL_ROOT=str(temp), WINLUX_POWER_SOCKET=str(temp / "power.sock"),
                WINLUX_LANG="fr", WINLUX_POWER_TEST_GUI_CRASH="1", PATH=str(tools))
    base.pop("DISPLAY", None)

    # 1. « now » : l'arrêt est décidé, l'écran plante, le gardien redémarre.
    marker = temp / "init-now.txt"
    init = fake_init(marker)
    result = subprocess.run([sys.executable, "-B", str(power), "now", "reboot"],
                            env=dict(base, WINLUX_POWER_INIT_PID=str(init.pid)),
                            capture_output=True, text=True, timeout=30)
    init.wait(10)
    assert result.returncode == 0, result
    assert marker.read_text().strip() == "reboot", result
    assert "sans écran" in result.stderr, result.stderr

    # 2. « ask » : l'écran plante avant la confirmation, rien ne s'arrête.
    marker = temp / "init-ask.txt"
    init = fake_init(marker)
    result = subprocess.run([sys.executable, "-B", str(power), "ask", "reboot"],
                            env=dict(base, WINLUX_POWER_INIT_PID=str(init.pid)),
                            capture_output=True, text=True, timeout=30)
    time.sleep(0.5)
    assert result.returncode != 0, result
    assert init.poll() is None and not marker.exists(), result
    init.terminate()
    init.wait(5)

# 3. L'installateur : « execute reboot » d'abord, l'écran seulement en cas de refus.
tree = ast.parse((here / "winlux-installer.py").read_text(encoding="utf-8"))
method = next(node for node in ast.walk(tree) if isinstance(node, ast.FunctionDef) and node.name == "on_reboot")
module = ast.Module(body=[method], type_ignores=[])


class Recorder:
    def __init__(self, execute_code):
        self.calls = []
        self.execute_code = execute_code

    def run(self, argv, **_kwargs):
        self.calls.append(("run", list(argv)))
        code = self.execute_code if argv[1:] == ["execute", "reboot"] else 0
        return subprocess.CompletedProcess(argv, code)

    def Popen(self, argv, **_kwargs):
        self.calls.append(("popen", list(argv)))


class Which:
    @staticmethod
    def which(name):
        return "/usr/bin/" + name


class Installer:
    quit_called = False

    def quit(self):
        self.quit_called = True


for code, expected in ((0, [("run", ["sync"]), ("run", ["/usr/bin/winlux-power", "execute", "reboot"])]),
                       (1, [("run", ["sync"]), ("run", ["/usr/bin/winlux-power", "execute", "reboot"]),
                            ("popen", ["/usr/bin/winlux-power", "now", "reboot"])])):
    recorder = Recorder(code)
    namespace = {"subprocess": recorder, "shutil": Which}
    exec(compile(module, "winlux-installer.py", "exec"), namespace)
    installer = Installer()
    namespace["on_reboot"](installer, None)
    assert recorder.calls == expected, recorder.calls
    assert installer.quit_called

print("test-power-reboot-guard : OK (écran planté, confirmation, installateur)")
