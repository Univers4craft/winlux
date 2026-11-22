# Un programme x86 obtient la liste des boutons de la barre des tâches
# (ITaskbarList 1 à 4) par CoCreateInstance, et sa progression arrive
# entière au runtime (#2973).
_sh = (ROOT / "runtime" / "src" / "wow32" / "com_shell.rs").read_text(encoding="utf-8")
ok(all(f'name: "{n}"' in _sh for n in ("ITaskbarList", "ITaskbarList2", "ITaskbarList3", "ITaskbarList4"))
   and "fn liste_de_la_barre_des_taches_2973" in _sh,
   "ITaskbarList, 2, 3 et 4 sont décrites pour les programmes x86 (#2973)")
