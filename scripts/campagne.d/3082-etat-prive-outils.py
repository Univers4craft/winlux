# #3082 : le dossier d'état ~/.local/state/winlux est partagé par nos outils
# et privé au compte. Quel que soit l'outil lancé le premier, sous umask 022,
# il naît en 0700 (un dossier déjà en 0755 y est ramené) ; les données du
# téléphone et les programmes récents du menu Démarrer naissent en 0600,
# relevé juste avant leur chmod ou leur remplacement (crochet d'audit).
import os as _os_3082
import stat as _st_3082
import sys as _sys_3082
import tempfile as _tmp_3082
_nes_3082, _actif_3082 = [], [False]
def _audit_3082(evenement, arguments):
    if _actif_3082[0] and evenement in ("os.chmod", "os.rename"):
        try:
            _p = _os_3082.fspath(arguments[0])
            if _os_3082.path.isfile(_p): _nes_3082.append((_p, _st_3082.S_IMODE(_os_3082.stat(_p).st_mode)))
        except (OSError, TypeError):
            pass
_sys_3082.addaudithook(_audit_3082)
def _mode_3082(p): return _st_3082.S_IMODE(p.stat().st_mode) if p.exists() else None

def _outil_3082(nom, action, lache=False):
    """Compte neuf (ou dossier winlux déjà en 0755 si lache) ; nom.py lancé, action(module, home)."""
    home = Path(_tmp_3082.mkdtemp(prefix=f"campagne-3082-{nom}-"))
    if lache:
        (home / ".local/state/winlux").mkdir(parents=True); (home / ".local/state/winlux").chmod(0o755)
    env = dict(_os_3082.environ)
    umask = _os_3082.umask(0o022)
    _nes_3082.clear()
    try:
        _os_3082.environ.update(HOME=str(home), XDG_STATE_HOME=str(home / ".local/state"),
                                XDG_CONFIG_HOME=str(home / ".config"), WINLUX_RUN_DIR=str(home / "run"),
                                WINLUX_LANG="fr", NO_AT_BRIDGE="1")
        module = runpy.run_path(str(SCRIPTS / f"{nom}.py"), run_name="campagne_3082")
        _actif_3082[0] = True
        action(module, home)
    finally:
        _actif_3082[0] = False
        _os_3082.umask(umask)
        _os_3082.environ.clear(); _os_3082.environ.update(env)
    return home, list(_nes_3082)

def _renommer_3082(m, home):
    (home / "doc").mkdir(); (home / "doc/a.txt").write_text("x")
    m["execute"]([(home / "doc/a.txt", "b.txt", None)])
def _start_3082(m, home):
    menu = m["Start"].__new__(m["Start"]); menu.state = {"recent": ["programme"]}; menu.save()

_cas_3082 = [
    ("winlux-rename", _renommer_3082, "rename-undo.json"),
    ("winlux-emoji", lambda m, h: m["recent_add"]("x"), "emoji-recent"),
    ("winlux-colorpicker", lambda m, h: m["save_history"](["#6A3DF0"]), "colors"),
    ("winlux-clipboard", lambda m, h: m["save"]([]), "clipboard/history.json"),
    ("winlux-alarms", lambda m, h: m["save"](m["load"]()), "clock.json"),
    ("winlux-update-page", lambda m, h: m["notify"](), "update-notice-seen"),
    ("winlux-phone", lambda m, h: m["write_json"](m["DATA"] / "essai.json", {"sms": "privé"}), "phone/essai.json"),
]
_resultats_3082 = {}
for _nom, _action, _fichier in _cas_3082:
    for _lache in (False, True):
        _home, _nes = _outil_3082(_nom, _action, _lache)
        _etat = _home / ".local/state/winlux"
        _resultats_3082[(_nom, _lache)] = (_mode_3082(_etat), (_etat / _fichier).exists(),
                                           [m for p, m in _nes if "/phone/" in p])
for _nom in ("winlux-start-menu", "winlux-settings-pages"):
    _action = _start_3082 if _nom == "winlux-start-menu" else (lambda m, h: m["update_start_menu_state"](recent=["programme"]))
    _home, _nes = _outil_3082(_nom, _action)
    _resultats_3082[(_nom, False)] = (_mode_3082(_home / ".config/winlux/start.json"),
                                      (_home / ".config/winlux/start.json").exists(), [m for p, m in _nes])
for _cle, _valeur in sorted(_resultats_3082.items()):
    print(f"#3082 : {_cle[0]}{' (dossier déjà 0755)' if _cle[1] else ''} : mode={oct(_valeur[0]) if _valeur[0] else None} "
          f"écrit={_valeur[1]} nés={[oct(m) for m in _valeur[2]]}")
_bons_3082 = all(v[0] == 0o700 and v[1] for (n, _), v in _resultats_3082.items() if n not in ("winlux-start-menu", "winlux-settings-pages"))
_bons_3082 &= all(m == 0o600 for (n, _), v in _resultats_3082.items() if n == "winlux-phone" for m in v[2])
_bons_3082 &= all(v[0] == 0o600 and v[1] and v[2] and all(m == 0o600 for m in v[2])
                  for (n, _), v in _resultats_3082.items() if n in ("winlux-start-menu", "winlux-settings-pages"))
ok(_bons_3082 and len(_resultats_3082) == 16,
   "Dossier d'état winlux en 0700 quel que soit l'outil lancé le premier, données nées en 0600 (#3082)")
