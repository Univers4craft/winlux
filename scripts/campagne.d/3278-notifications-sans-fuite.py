# #3278 : le démon de notifications sert ses appels D-Bus par un filtre de
# connexion, sans register_object dont chaque appel gardait l'invocation
# (environ 3 Ko par notification, sans plafond). Ici : le démon s'inscrit
# par le filtre seul, prend les appels de son objet et laisse passer les
# autres, et ses réponses sont construites à partir du message reçu.
# Mesure de mémoire sous Xvfb : scripts/test-notifications-memoire.py.
import os as _os_3278
import tempfile as _tempfile_3278
_env_3278 = dict(_os_3278.environ)
try:
    _os_3278.environ.update(XDG_STATE_HOME=_tempfile_3278.mkdtemp(prefix="campagne-3278-"),
                            XDG_CONFIG_HOME=_tempfile_3278.mkdtemp(prefix="campagne-3278-"), WINLUX_LANG="fr")
    _demon_3278 = runpy.run_path(str(SCRIPTS / "winlux-notifications.py"), run_name="campagne_3278")
finally:
    _os_3278.environ.clear()
    _os_3278.environ.update(_env_3278)
_Gio_3278, _GLib_3278 = _demon_3278["Gio"], _demon_3278["GLib"]


class _Connexion3278:
    def __init__(self):
        self.filtres, self.objets, self.envois = [], [], []
    def add_filter(self, fonction):
        self.filtres.append(fonction)
    def register_object(self, *args):
        self.objets.append(args)
    def send_message(self, message, _flags):
        self.envois.append(message)


def _appel_3278(interface, membre, chemin="/org/freedesktop/Notifications"):
    message = _Gio_3278.DBusMessage.new_method_call(":1.9", chemin, interface, membre)
    message.set_serial(7)  # un appel reçu porte toujours un numéro
    return message


_c_3278 = _Connexion3278()
_pris_3278 = _laisse_3278 = False
_reponses_3278 = []
try:
    _n_3278 = _demon_3278["Notifications"].__new__(_demon_3278["Notifications"])
    _n_3278.acquired(_c_3278, None)
    _pris_3278 = _n_3278.incoming(_c_3278, _appel_3278("org.freedesktop.Notifications", "GetServerInformation"), True) is None
    _autre_3278 = _appel_3278("org.freedesktop.Notifications", "GetServerInformation", "/ailleurs")
    _peer_3278 = _appel_3278("org.freedesktop.DBus.Peer", "Ping")
    _laisse_3278 = (_n_3278.incoming(_c_3278, _autre_3278, True) is _autre_3278
                    and _n_3278.incoming(_c_3278, _peer_3278, True) is _peer_3278)
    _n_3278.dispatch(_c_3278, _appel_3278("org.freedesktop.Notifications", "GetServerInformation"))
    _mauvais_3278 = _appel_3278("org.freedesktop.Notifications", "CloseNotification")
    _mauvais_3278.set_body(_GLib_3278.Variant("(s)", ("x",)))
    _n_3278.dispatch(_c_3278, _mauvais_3278)
    _reponses_3278 = [(m.get_message_type(), m.get_error_name(), m.get_body() and m.get_body().unpack())
                      for m in _c_3278.envois]
except Exception as _erreur_3278:  # démon d'avant #3278 : contrôle en échec, pas campagne arrêtée
    print(f"#3278 : {_erreur_3278!r}")
print(f"#3278 : filtres={len(_c_3278.filtres)} objets={len(_c_3278.objets)} pris={_pris_3278} "
      f"laissés={_laisse_3278} réponses={_reponses_3278}")
ok(len(_c_3278.filtres) == 1 and not _c_3278.objets and _pris_3278 and _laisse_3278
   and len(_reponses_3278) == 2 and _reponses_3278[0] == (_Gio_3278.DBusMessageType.METHOD_RETURN, None, ("Winlux", "Winlux", "1.0", "1.2"))
   and _reponses_3278[1][1] == "org.freedesktop.DBus.Error.InvalidArgs"
   and (SCRIPTS / "test-notifications-memoire.py").exists(),
   "Notifications : appels D-Bus servis sans garder d'invocation, mémoire stable (#3278)")
