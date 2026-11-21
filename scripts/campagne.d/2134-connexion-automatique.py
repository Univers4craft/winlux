# Connexion automatique jamais par défaut : case décochée à l'installation, /init ne l'active que sur "autologin": true (#2134).
_i = (ROOT / "scripts/winlux-installer.py").read_text(encoding="utf-8")
_b = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
ok('"autologin": False,' in _i and '"autologin": True' not in _i,
   "installateur : connexion automatique décochée par défaut (#2134)")
# Comportement lu dans l'arbre syntaxique : tout appel set_active() sur la case autologin passe False.
import ast as _ast
_coches = [n.args[0].value for n in _ast.walk(_ast.parse(_i))
           if isinstance(n, _ast.Call) and isinstance(n.func, _ast.Attribute) and n.func.attr == "set_active"
           and isinstance(n.func.value, _ast.Name) and n.func.value.id == "autologin"
           and n.args and isinstance(n.args[0], _ast.Constant)]
ok(_coches and not any(_coches),
   "installateur : la case n'est jamais recochée d'office (#2134)")
ok("desktop_autologin=0\n" in _b and "desktop_autologin=1\n" not in _b
   and '"autologin":[[:space:]]*true\' /etc/winlux/install-config.json; then desktop_autologin=1; fi' in _b,
   "/init : mot de passe demandé sauf choix explicite \"autologin\": true (#2134)")
