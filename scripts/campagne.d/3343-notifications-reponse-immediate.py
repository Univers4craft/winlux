# #3343 : Notify répond avant d'écrire l'historique et avant la bulle, et une
# rafale de notifications ne donne qu'une écriture de l'historique, pas une
# par notification. Mesure de temps sous Xvfb : scripts/test-notifications-latence.py.
import os as _os_3343
import tempfile as _tempfile_3343
_etat_3343 = Path(_tempfile_3343.mkdtemp(prefix="campagne-3343-"))
_env_3343 = dict(_os_3343.environ)
try:
    _os_3343.environ.update(XDG_STATE_HOME=str(_etat_3343), XDG_CONFIG_HOME=str(_etat_3343 / "config"), WINLUX_LANG="fr")
    _demon_3343 = runpy.run_path(str(SCRIPTS / "winlux-notifications.py"), run_name="campagne_3343")
finally:
    _os_3343.environ.clear()
    _os_3343.environ.update(_env_3343)
_Gio_3343, _GLib_3343 = _demon_3343["Gio"], _demon_3343["GLib"]
_journal_3343 = []
_resultat_3343 = None


class _Connexion3343:
    def send_message(self, message, _flags):
        _journal_3343.append("réponse")
    def emit_signal(self, *_):
        pass


try:
    (_etat_3343 / "config/winlux").mkdir(parents=True)
    (_etat_3343 / "config/winlux/do-not-disturb").touch()   # sans bulle : seul l'ordre réponse / écriture compte
    _n_3343 = _demon_3343["Notifications"].__new__(_demon_3343["Notifications"])
    _n_3343.__dict__.update(history=[], sequence=0, live={}, toast=None)
    _ecrire_3343 = _n_3343.save
    _n_3343.save = lambda: (_journal_3343.append("écriture"), _ecrire_3343())
    _c_3343 = _Connexion3343()
    for _i_3343 in range(100):
        _m_3343 = _Gio_3343.DBusMessage.new_method_call(":1.9", "/org/freedesktop/Notifications",
                                                        "org.freedesktop.Notifications", "Notify")
        _m_3343.set_serial(_i_3343 + 1)
        _m_3343.set_body(_GLib_3343.Variant("(susssasa{sv}i)", ("a", 0, "", f"t{_i_3343}", "x", [], {}, 100)))
        _n_3343.dispatch(_c_3343, _m_3343)
    _avant_3343 = list(_journal_3343)
    _contexte_3343 = _GLib_3343.MainContext.default()
    while _contexte_3343.pending():
        _contexte_3343.iteration(False)
    _fichier_3343 = _etat_3343 / "winlux/notifications.json"
    _resultat_3343 = (_avant_3343.count("réponse"), _avant_3343.count("écriture"), _journal_3343.count("écriture"),
                      _fichier_3343.exists() and len(__import__("json").loads(_fichier_3343.read_text())))
except Exception as _erreur_3343:  # démon d'avant #3343 : contrôle en échec, pas campagne arrêtée
    print(f"#3343 : {_erreur_3343!r}")
print(f"#3343 : réponses, écritures pendant la rafale, écritures en tout, entrées écrites = {_resultat_3343}")
ok(_resultat_3343 == (100, 0, 1, 100) and (SCRIPTS / "test-notifications-latence.py").exists(),
   "Notifications : Notify répond aussitôt, une rafale donne une seule écriture de l'historique (#3343)")
