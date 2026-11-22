# #2874 : en français, le fil d'Ariane traduit aussi les dossiers connus du
# compte (Utilisateurs > nom > Images), pas seulement « Users ».
_e_2874 = (ROOT / "testexe/src/shell/explorer.rs").read_text(encoding="utf-8")
ok("fn translate_crumbs" in _e_2874,
   "Explorateur : fil d'Ariane Utilisateurs > nom > Images en français (#2874)")
