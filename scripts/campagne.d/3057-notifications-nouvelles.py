# #3057 : la pastille de la cloche compte les notifications nouvelles, pas
# tout l'historique. Arrivée = nouvelle ; bulle fermée = vue ; centre ouvert
# (MarkSeen) = tout vu ; la cloche lit le compte, privé au compte (0600).
# Parcours complet (démon, bus, centre ouvert) : scripts/test-notification-nouvelles.py.
import os as _os_3057
import stat as _stat_3057
import tempfile as _tempfile_3057
_etat_3057 = Path(_tempfile_3057.mkdtemp(prefix="campagne-3057-"))
_env_3057 = dict(_os_3057.environ)
try:
    _os_3057.environ.update(XDG_STATE_HOME=str(_etat_3057), WINLUX_LANG="fr")
    _demon_3057 = runpy.run_path(str(SCRIPTS / "winlux-notifications.py"), run_name="campagne_3057")
    _cloche_3057 = runpy.run_path(str(SCRIPTS / "winlux-tray.py"), run_name="campagne_3057")
    _n_3057 = _demon_3057["Notifications"].__new__(_demon_3057["Notifications"])
    _n_3057.history = [dict(id=1, app="a", title="vue", body="", actions=[], time=0)]
    _n_3057.save()
    _vide_3057 = _cloche_3057["notification_count"]()
    _n_3057.history += [dict(id=i, app="a", title="t", body="", actions=[], time=0, nouvelle=True) for i in (2, 3, 4)]
    _n_3057.save()
    _trois_3057 = _cloche_3057["notification_count"]()
    _n_3057.mark_seen(3)
    _bulle_3057 = _cloche_3057["notification_count"]()
    _n_3057.mark_seen()
    _centre_3057 = _cloche_3057["notification_count"]()
    _mode_3057 = _stat_3057.S_IMODE((_etat_3057 / "winlux/notifications-nouvelles").stat().st_mode)
    _gardees_3057 = len(_n_3057.history)
finally:
    _os_3057.environ.clear()
    _os_3057.environ.update(_env_3057)
print(f"#3057 : vide={_vide_3057} trois={_trois_3057} bulle={_bulle_3057} centre={_centre_3057} "
      f"mode={oct(_mode_3057)} gardées={_gardees_3057}")
ok((_vide_3057, _trois_3057, _bulle_3057, _centre_3057, _mode_3057, _gardees_3057) == (0, 3, 2, 0, 0o600, 4)
   and (SCRIPTS / "test-notification-nouvelles.py").exists(),
   "Cloche : la pastille compte les notifications nouvelles, éteinte une fois vues, historique gardé (#3057)")
