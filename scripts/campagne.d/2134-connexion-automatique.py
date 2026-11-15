# Connexion automatique jamais par défaut : case décochée à l'installation, /init ne l'active que sur "autologin": true (#2134).
_i = (ROOT / "scripts/winlux-installer.py").read_text(encoding="utf-8")
_b = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
ok('"autologin": False,' in _i and '"autologin": True' not in _i,
   "installateur : connexion automatique décochée par défaut (#2134)")
_c = _i[_i.index('autologin = Gtk.CheckButton(label=tr("autologin"))'):]
ok("autologin.set_active(False)" in _c[:400] and "autologin.set_active(True)" not in _i,
   "installateur : la case n'est jamais recochée d'office (#2134)")
ok("desktop_autologin=0\n" in _b and "desktop_autologin=1\n" not in _b
   and '"autologin":[[:space:]]*true\' /etc/winlux/install-config.json; then desktop_autologin=1; fi' in _b,
   "/init : mot de passe demandé sauf choix explicite \"autologin\": true (#2134)")
