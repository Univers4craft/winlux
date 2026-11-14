# Bouton d'alimentation pendant le verrouillage : la carte d'arrêt reste atteignable (#2254).
import ast as _ast
_p = (ROOT / "scripts/winlux-power.py").read_text(encoding="utf-8")
_l = (ROOT / "scripts/winlux-lock.py").read_text(encoding="utf-8")
_d = (ROOT / "runtime/src/power/daemon.rs").read_text(encoding="utf-8")
ok("request_lock_yield()" in _p and "def watch_power(self):" in _l, "écran d'arrêt : saisie cédée par le verrou (#2254)")
ok("def power_request_pid():" in _l and "allowed = {0, os.getuid()}" in _l, "verrou : ne cède qu'à un vrai winlux-power de root ou du compte (#2254)")
ok('log(f"saisie refusée' in _p, "écran d'arrêt : échec de saisie journalisé (#2254)")
ok("if self.locked else title" in _p, "écran verrouillé : noms d'applications, jamais les titres de fenêtres (#2254)")

# Aucun arrêt automatique au bout d'un délai (#2254, contrôle réécrit pour
# #2299) : lu dans l'arbre syntaxique de winlux-power.py, pas dans son texte.
# 1. Quand le délai de fermeture expire (branche qui teste self.deadline),
#    wait_windows montre la carte « bloqué » et n'appelle pas self.finish.
# 2. show_blocked n'appelle jamais self.finish : il ne le passe qu'en
#    rappel d'un bouton.
# 3. Aucune minuterie GLib (timeout_add*) n'a self.finish pour rappel.
_tree = _ast.parse(_p)
_cls = next(n for n in _tree.body if isinstance(n, _ast.ClassDef) and n.name == "PowerUI")
_meth = {n.name: n for n in _cls.body if isinstance(n, _ast.FunctionDef)}


def _is_self(node, attr):
    return (isinstance(node, _ast.Attribute) and node.attr == attr
            and isinstance(node.value, _ast.Name) and node.value.id == "self")


def _calls(node, attr):
    return any(isinstance(n, _ast.Call) and _is_self(n.func, attr) for n in _ast.walk(node))


_deadline = [n for n in _ast.walk(_meth["wait_windows"]) if isinstance(n, _ast.If)
             and any(_is_self(t, "deadline") for t in _ast.walk(n.test))]
_tuples = [n for n in _ast.walk(_meth["show_blocked"]) if isinstance(n, _ast.Tuple)]
_timers = [n for n in _ast.walk(_tree) if isinstance(n, _ast.Call)
           and isinstance(n.func, _ast.Attribute) and n.func.attr.startswith("timeout_add")]
ok(len(_deadline) == 1
   and any(_calls(s, "show_blocked") for s in _deadline[0].body)
   and not any(_calls(s, "finish") for s in _deadline[0].body)
   and not _calls(_meth["show_blocked"], "finish")
   and any(any(_is_self(e, "finish") for e in t.elts) for t in _tuples)
   and not any(any(_is_self(a, "finish") for a in t.args) for t in _timers),
   "aucun arrêt automatique au bout d'un délai, choix laissé à la personne (#2254)")
ok("display_on(root);" in _d[_d.index("Hw::PowerButton => {"):][:400], "bouton d'alimentation : écran rallumé (#2254)")
ok((ROOT / "scripts/test-all.d/2254-arret-sous-verrou.sh").exists(), "test hors ligne de la passation (#2254)")
ok("os.fchmod(marker.fileno(), 0o644)" in _p, "demande d'arrêt lisible par le compte verrouillé malgré l'umask 077 du démon (#2254)")
