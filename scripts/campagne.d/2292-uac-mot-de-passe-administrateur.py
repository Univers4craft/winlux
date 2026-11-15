# UAC d'un compte standard : le mot de passe de l'administrateur choisi est vérifié par le relais root (#2292).
_u = (ROOT / "scripts/winlux-uac.py").read_text(encoding="utf-8")
_v = _u[_u.index("def verify("):_u.index("def run_dialog(")]
ok('relay_check"](user, password, "admin-check")' in _v, "winlux-uac : un autre compte que la session est vérifié par le relais root (#2292)")
_r = (ROOT / "scripts/winlux-root.py").read_text(encoding="utf-8")
ok('("elevate", "admin-check") and not is_admin(user)' in _r, "winlux-root : admin-check n'accepte qu'un administrateur (#2292)")
ok('if request.get("op") == "elevate":\n            ELEVATED[uid]' in _r, "winlux-root : admin-check n'ouvre pas d'élévation (#2292)")
