# #3060 : l'historique des notifications est privé. Dossier d'état créé en
# 0700 (un dossier winlux déjà ouvert en 0755 y est ramené), et chaque
# fichier naît en 0600 : relevé juste avant son chmod ou son remplacement.
import os as _os_3060
import stat as _stat_3060
import tempfile as _tempfile_3060
_etat_3060 = Path(_tempfile_3060.mkdtemp(prefix="campagne-3060-"))
_env_3060 = dict(_os_3060.environ)
_umask_3060 = _os_3060.umask(0o022)
# Juste avant chaque chmod ou remplacement d'un fichier (crochet d'audit,
# donc avant que l'appel agisse) : mode réel de ce fichier à cet instant.
_nes_3060 = []
_actif_3060 = [False]
def _releve_3060(evenement, arguments):
    if _actif_3060[0] and evenement in ("os.chmod", "os.rename"):
        try:
            _chemin_3060 = _os_3060.fspath(arguments[0])
            if _os_3060.path.isfile(_chemin_3060):
                _nes_3060.append(_stat_3060.S_IMODE(_os_3060.stat(_chemin_3060).st_mode))
        except (OSError, TypeError):
            pass
__import__("sys").addaudithook(_releve_3060)
try:
    _os_3060.environ.update(XDG_STATE_HOME=str(_etat_3060 / "neuf"))
    _demon_3060 = runpy.run_path(str(SCRIPTS / "winlux-notifications.py"), run_name="campagne_3060")
    _n_3060 = _demon_3060["Notifications"].__new__(_demon_3060["Notifications"])
    _n_3060.history = [dict(id=1, app="a", title="message privé", body="", actions=[], time=0, nouvelle=True)]
    _actif_3060[0] = True
    _n_3060.save()
    _actif_3060[0] = False
    _neuf_3060 = [_stat_3060.S_IMODE(p.stat().st_mode) for p in
                  (_etat_3060 / "neuf", _etat_3060 / "neuf/winlux", _etat_3060 / "neuf/winlux/notifications.json")]
    (_etat_3060 / "ancien/winlux").mkdir(parents=True)
    (_etat_3060 / "ancien/winlux").chmod(0o755)
    _os_3060.environ["XDG_STATE_HOME"] = str(_etat_3060 / "ancien")
    _demon_3060 = runpy.run_path(str(SCRIPTS / "winlux-notifications.py"), run_name="campagne_3060")
    _n_3060 = _demon_3060["Notifications"].__new__(_demon_3060["Notifications"])
    _n_3060.history = []
    _n_3060.save()
    _ancien_3060 = _stat_3060.S_IMODE((_etat_3060 / "ancien/winlux").stat().st_mode)
    _restes_3060 = sorted(p.name for p in (_etat_3060 / "neuf/winlux").iterdir())
finally:
    _actif_3060[0] = False
    _os_3060.umask(_umask_3060)
    _os_3060.environ.clear()
    _os_3060.environ.update(_env_3060)
print(f"#3060 : neuf={[oct(m) for m in _neuf_3060]} nés={[oct(m) for m in _nes_3060]} "
      f"ancien={oct(_ancien_3060)} fichiers={_restes_3060}")
ok(_neuf_3060 == [0o700, 0o700, 0o600] and _nes_3060 and all(m == 0o600 for m in _nes_3060)
   and _ancien_3060 == 0o700 and _restes_3060 == ["notifications-nouvelles", "notifications.json"],
   "Notifications : dossier d'état en 0700, fichiers nés en 0600, aucun temporaire laissé (#3060)")
