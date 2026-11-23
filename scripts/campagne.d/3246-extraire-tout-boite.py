# #3246 : « Extraire tout… » ouvre une boîte (destination modifiable,
# Parcourir…, case « Afficher les fichiers extraits… » cochée, Extraire /
# Annuler) au lieu d'extraire aussitôt. Comportement vérifié par
# explorer_regression (contrôles nommés ci-dessous).
_ex_3246 = (ROOT / "testexe/src/shell/explorer_extract.rs").read_text(encoding="utf-8")
_reg_3246 = (ROOT / "testexe/src/shell/explorer.rs").read_text(encoding="utf-8")
ok("boite ouverte, rien d'extrait, destination preremplie, case cochee (#3246)" in _ex_3246
   and "Echap annule sans rien lancer (#3246)" in _ex_3246
   and "extract::regression();" in _reg_3246,
   "« Extraire tout… » ouvre une boîte avant d'extraire, Échap annule (#3246)")
