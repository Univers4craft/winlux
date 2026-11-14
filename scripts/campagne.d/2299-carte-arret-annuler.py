# Carte « Ces applications empêchent l'arrêt » : « Annuler » par défaut ; sur
# un écran verrouillé, la touche qui réveille l'écran éteignait l'ordinateur (#2299).
# Lu dans l'arbre syntaxique, pas dans le texte (#2349) : make_card donne le
# focus au premier bouton dont le troisième champ (accent) est vrai ; dans
# show_blocked, ce bouton doit avoir self.cancel pour rappel, et self.finish ne
# doit jamais être le bouton par défaut. Le comportement lui-même (focus réel)
# est vérifié sous Xvfb par test-power-carte-annuler.py.
import ast as _ast
_p = (ROOT / "scripts/winlux-power.py").read_text(encoding="utf-8")
_cls = next(n for n in _ast.parse(_p).body if isinstance(n, _ast.ClassDef) and n.name == "PowerUI")
_sb = next(n for n in _cls.body if isinstance(n, _ast.FunctionDef) and n.name == "show_blocked")


def _self_attr(node):
    if isinstance(node, _ast.Attribute) and isinstance(node.value, _ast.Name) and node.value.id == "self":
        return node.attr
    return None


_buttons = [e for n in _ast.walk(_sb) if isinstance(n, _ast.List) for e in n.elts
            if isinstance(e, _ast.Tuple) and len(e.elts) == 3]
_defaults = [b for b in _buttons if isinstance(b.elts[2], _ast.Constant) and b.elts[2].value]
ok(bool(_defaults) and _self_attr(_defaults[0].elts[1]) == "cancel"
   and not any(_self_attr(b.elts[1]) == "finish" for b in _defaults),
   "carte d'arrêt bloquée : « Annuler » par défaut (#2299)")
_ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
ok((ROOT / "scripts/test-power-carte-annuler.py").exists() and "scripts/test-power-carte-annuler.py" in _ci,
   "test du focus de la carte d'arrêt sous Xvfb, lancé par la CI (#2299)")
