# #3454 : par défaut, ni veille, ni écran éteint, ni économiseur, ni
# verrouillage automatique sur inactivité, sur secteur comme sur batterie.
_m = (ROOT / "runtime/src/power/mod.rs").read_text(encoding="utf-8")
_t = (ROOT / "runtime/src/power/tests.rs").read_text(encoding="utf-8")
_x = (ROOT / "integration/native/xscreensaver").read_text(encoding="utf-8")
_l = (ROOT / "scripts/winlux-lock.py").read_text(encoding="utf-8")
_e = (ROOT / "scripts/test-ecran-veille-retour.py").read_text(encoding="utf-8")
ok("fn jamais_de_veille_par_defaut(" in _t and "pub const OLD_DEFAULT_DELAYS" in _m,
   "alimentation : jamais de veille ni d'écran éteint par défaut, anciens défauts migrés (#3454)")
ok("(120, 0)" in _t, "mode Économie : « Jamais » n'est plus plafonné (#3454)")
ok("mode:           off" in _x and "#3454" in _e, "XScreenSaver : pas d'économiseur sur inactivité, comptes migrés (#3454)")
ok('conf = {"LOCK_AFTER": "0"' in _l, "verrouillage automatique : jamais par défaut (#3454)")
