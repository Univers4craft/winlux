# #3121 (gain) : le menu Démarrer lit le catalogue des applications une fois
# (préchargement) et ne le relit que sur le signal « changed » de
# Gio.AppInfoMonitor, au repos quand le menu est caché.
_menu_3121 = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="campagne_3121")
_gio_3121, _glib_3121 = _menu_3121["Gio"], _menu_3121["GLib"]
_lectures_3121 = [0]
_get_all_3121 = _gio_3121.AppInfo.get_all
def _compte_3121():
    _lectures_3121[0] += 1
    return []
_gio_3121.AppInfo.get_all = staticmethod(_compte_3121)
try:
    _app_3121 = _menu_3121["Start"]()
    _neuf_3121 = _app_3121.apps_stale
    _app_3121.load_apps()
    _apres_lecture_3121 = (_lectures_3121[0], _app_3121.apps_stale)
    _app_3121.apps_changed(None)
    _marque_3121 = _app_3121.apps_stale
    _ctx_3121 = _glib_3121.MainContext.default()
    for _ in range(50):
        if not _ctx_3121.iteration(False): break
    _relu_3121 = (_lectures_3121[0], _app_3121.apps_stale)
finally:
    _gio_3121.AppInfo.get_all = _get_all_3121
print(f"      neuf : à lire={_neuf_3121} ; après load_apps : {_apres_lecture_3121} ; signal : à lire={_marque_3121} ; relu au repos : {_relu_3121}")
ok(_neuf_3121 and _apres_lecture_3121 == (1, False) and _marque_3121 and _relu_3121 == (2, False),
   "Démarrer : catalogue des applications relu sur signal de GIO, au repos, pas à chaque ouverture (#3121)")
