# #3341 : le service de veille de l'écran sert ses appels D-Bus par un filtre
# de connexion, sans register_object dont chaque appel gardait l'invocation
# (environ 2,3 Ko par appel). Ici : le filtre prend les appels de ses deux
# chemins et laisse passer le reste ; la réponse part du message reçu.
# Mesure de mémoire sur un bus de test : scripts/test-veille-dbus-memoire.py.
_veille_3341 = runpy.run_path(str(SCRIPTS / "winlux-screensaver-inhibit.py"), run_name="campagne_3341")
_pris_3341 = _laisse_3341 = False
_reponses_3341, _appels_3341 = [], []
try:
    from gi.repository import Gio as _Gio_3341, GLib as _GLib_3341

    class _Connexion3341:
        envois = []
        def send_message(self, message, _flags):
            self.envois.append(message)

    def _appel_3341(chemin, interface, membre, corps=None):
        message = _Gio_3341.DBusMessage.new_method_call(":1.9", chemin, interface, membre)
        message.set_serial(7)
        if corps is not None:
            message.set_body(corps)
        return message

    _filtre_3341 = _veille_3341["bus_filter"]({"Inhibit": "ss", "GetActive": ""},
                                             lambda *args: (_appels_3341.append(args[4]),
                                                            args[6].return_value(_GLib_3341.Variant("(u)", (5,)))))
    _c_3341 = _Connexion3341()
    _pris_3341 = all(_filtre_3341(_c_3341, _appel_3341(p, "org.freedesktop.ScreenSaver", "GetActive"), True) is None
                     for p in ("/org/freedesktop/ScreenSaver", "/ScreenSaver"))
    _autre_3341 = _appel_3341("/ailleurs", "org.freedesktop.ScreenSaver", "GetActive")
    _peer_3341 = _appel_3341("/ScreenSaver", "org.freedesktop.DBus.Peer", "Ping")
    _laisse_3341 = (_filtre_3341(_c_3341, _autre_3341, True) is _autre_3341
                    and _filtre_3341(_c_3341, _peer_3341, True) is _peer_3341)
    _contexte_3341 = _GLib_3341.MainContext.default()
    while _contexte_3341.pending():
        _contexte_3341.iteration(False)
    _filtre_3341(_c_3341, _appel_3341("/ScreenSaver", "org.freedesktop.ScreenSaver", "Inhibit",
                                      _GLib_3341.Variant("(ss)", ("a", "b"))), True)
    _filtre_3341(_c_3341, _appel_3341("/ScreenSaver", "org.freedesktop.ScreenSaver", "Inhibit",
                                      _GLib_3341.Variant("(u)", (1,))), True)
    while _contexte_3341.pending():
        _contexte_3341.iteration(False)
    _reponses_3341 = [(m.get_error_name(), m.get_body() and m.get_body().unpack()) for m in _c_3341.envois]
except Exception as _erreur_3341:  # service d'avant #3341 : contrôle en échec, pas campagne arrêtée
    print(f"#3341 : {_erreur_3341!r}")
print(f"#3341 : pris={_pris_3341} laissés={_laisse_3341} appels={_appels_3341} réponses={_reponses_3341}")
ok(_pris_3341 and _laisse_3341 and _appels_3341 == ["GetActive", "GetActive", "Inhibit"]
   and _reponses_3341[:3] == [(None, (5,))] * 3
   and _reponses_3341[3][0] == "org.freedesktop.DBus.Error.InvalidArgs"
   and (SCRIPTS / "test-veille-dbus-memoire.py").exists(),
   "Veille de l'écran : appels D-Bus servis sans garder d'invocation, mémoire stable (#3341)")
