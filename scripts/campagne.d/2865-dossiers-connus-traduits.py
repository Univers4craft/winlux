# #2865 : C:\Users\<nom>\Pictures (et la famille Desktop, Music, Videos…) est
# le dossier traduit existant (~/Images) quand user-dirs.dirs n'a pas la clé ;
# le test unitaire vérifie les deux sens de la correspondance.
_p_2865 = (ROOT / "runtime/src/paths/tests.rs").read_text(encoding="utf-8")
ok("fn dossiers_connus_francais_sans_cle_xdg" in _p_2865,
   "Dossiers connus : Pictures est ~/Images sur un compte français sans clé XDG (#2865)")
