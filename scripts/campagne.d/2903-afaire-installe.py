# #2903 : un installateur mené à bien par winlux-open (installateur reconnu
# et paquet .msi) est noté pour le compte, et le bloc « À faire » du menu
# Démarrer ne le propose plus ; un installateur annulé reste proposé.
_d_2903 = Path(tempfile.mkdtemp(prefix="campagne-2903-"))
_dl_2903 = _d_2903 / "Téléchargements"
_dl_2903.mkdir()
_faux_2903 = _d_2903 / "winlux-faux"
_faux_2903.write_text('#!/bin/sh\ncase "$1" in --inspect) echo setup; exit 0 ;; --setup) exit 1 ;; esac\n'
                      'case "$*" in *annule*) exit 66 ;; esac\nexit 0\n')
_faux_2903.chmod(0o755)
for _nom in ("npp.8.6.Installer.x64.exe", "putty-64bit-0.81-installer.msi", "annule-setup.exe"):
    (_dl_2903 / _nom).write_bytes(b"MZ")
    _env = {k: v for k, v in os.environ.items() if k not in ("DISPLAY", "WAYLAND_DISPLAY")}
    _env.update(WINLUX_RUNTIME=str(_faux_2903), WINLUX_NO_TERMINAL="1", WINLUX_GUI="headless",
                XDG_STATE_HOME=str(_d_2903 / "etat"), HOME=str(_d_2903), TMPDIR=str(_d_2903))
    subprocess.run(["sh", str(ROOT / "integration/winlux-open"), str(_dl_2903 / _nom)], env=_env,
                   stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=60)
_menu_2903 = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="campagne_2903")
import time as _time_2903
_items_2903 = _menu_2903["todo_items"](_dl_2903, set(), set(), _time_2903.time(), limit=5,
                                       done=_menu_2903["installers_done"](_d_2903 / "etat"))
ok([i["title"] for i in _items_2903 if i["kind"] == "installer"] == ["annule-setup.exe"],
   "Démarrer, « À faire » : installateurs installés retirés, l'annulé reste proposé (#2903)")
