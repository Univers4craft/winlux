# #2855 : LoadIcon(NULL, IDI_*) rend un signe distinct pour erreur,
# question, avertissement, information et bouclier. Le comportement est
# gardé par les tests unitaires nommés.
src = (ROOT / "runtime" / "src" / "dlls" / "icons.rs").read_text(encoding="utf-8")
ok("fn icones_systeme_distinctes_2855" in src
   and "fn load_icon_idi_error_rend_le_signe_d_erreur_2855" in src,
   "Icônes système : un signe distinct par IDI_* (#2855)")
