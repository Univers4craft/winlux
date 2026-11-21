# UAC d'un compte standard : le mot de passe de l'administrateur choisi est vérifié par le relais root (#2292).
_u = (ROOT / "scripts/winlux-uac.py").read_text(encoding="utf-8")
_v = _u[_u.index("def verify("):_u.index("def run_dialog(")]
ok('relay_check"](user, password, "admin-check")' in _v, "winlux-uac : un autre compte que la session est vérifié par le relais root (#2292)")
_r = (ROOT / "scripts/winlux-root.py").read_text(encoding="utf-8")
# Comportement vérifié par test-root.py (CI), sur un vrai relais : libellés de ses vérifications.
_tr = (ROOT / "scripts/test-root.py").read_text(encoding="utf-8")
_ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
ok("def is_admin(" in _r and "compte standard refusé par admin-check" in _tr and "scripts/test-root.py" in _ci, "winlux-root : admin-check n'accepte qu'un administrateur (#2292)")
ok("admin-check n'ouvre pas d'élévation" in _tr and "scripts/test-root.py" in _ci, "winlux-root : admin-check n'ouvre pas d'élévation (#2292)")
