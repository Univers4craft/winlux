# #1268 : défauts des propriétés liées posés après Initialize (CheckerboardEffect de Paint.NET).
_src = (runtime_dlls / "d2d1_effects.rs").read_text(encoding="utf-8")
ok(_src.find("let hr = init(imp, context.cast(), graph.cast());") < _src.find("Valeurs par défaut des propriétés liées")
   and "valeurs_par_defaut_posees_apres_initialize" in (SCRIPTS / "test-all.d" / "1268-effet-personnalise-defauts.sh").read_text(encoding="utf-8"),
   "Effet personnalisé : défauts posés après Initialize (#1268)")
