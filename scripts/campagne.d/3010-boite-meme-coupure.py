# #3010 : une boîte de message prévoit sa hauteur avec la même coupure de
# lignes que la statique qui affiche son texte ; le dernier mot ne tombe
# plus sur une ligne coupée. Le comportement est gardé par le test
# unitaire nommé.
src = (ROOT / "runtime" / "src" / "dlls" / "dialogs" / "tests.rs").read_text(encoding="utf-8")
ok("fn boite_de_message_meme_coupure_que_sa_statique_3010" in src,
   "Boîte de message : même coupure de lignes que sa statique (#3010)")
