# #2756 : après un débordement de pile, le rapport relève les appelants au-
# dessus de la page de garde. Comportement gardé par le test unitaire
# crash::tests::debordement_rapporte_les_appelants (vrai débordement dans un
# processus enfant).
crash_2756 = (ROOT / "runtime" / "src" / "crash.rs").read_text(encoding="utf-8")
ok("fn first_readable" in crash_2756 and "fn debordement_rapporte_les_appelants" in crash_2756,
   "rapport de débordement de pile : appelants relevés (#2756)")
