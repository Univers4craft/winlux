# UAC d'un compte standard : le mot de passe d'un administrateur installé (users.conf « shadow ») est vérifié par le relais root (#2415).
_t = (ROOT / "scripts/test-uac.py").read_text(encoding="utf-8")
ok("#2415" in _t and "administrateur installé accepté" in _t, "test : administrateur installé accepté par admin-check (#2415)")
