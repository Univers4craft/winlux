# #2756 : après un débordement de pile, le rapport relève les appelants au-
# dessus de la page de garde. Comportement gardé par le test unitaire
# crash::tests::appelants_releves_au_dessus_de_la_garde (pile préparée avec
# une page de garde, indépendante du compilateur).
crash_2756 = (ROOT / "runtime" / "src" / "crash.rs").read_text(encoding="utf-8")
ok("fn first_readable" in crash_2756 and "fn appelants_releves_au_dessus_de_la_garde" in crash_2756,
   "rapport de débordement de pile : appelants relevés (#2756)")
