# Réglages valables pour tout l'appareil : un compte standard passe par l'UAC (#2269).
import ast as _ast
_tree = _ast.parse((ROOT / "scripts/winlux-root.py").read_text(encoding="utf-8"))
_machine = next((_ast.literal_eval(n.value) for n in _tree.body if isinstance(n, _ast.Assign)
                 and any(getattr(t, "id", "") == "CONF_MACHINE" for t in n.targets)), set())
ok("privacy.conf" in _machine, "privacy.conf demande un administrateur au compte standard (#2269)")
ok("locale.conf" not in _machine, "langue d'affichage toujours sans élévation (#2269)")
_t = (ROOT / "scripts/test-root.py").read_text(encoding="utf-8")
ok("#2269" in _t, "test du relais : refus sans élévation, accord après élévation (#2269)")
