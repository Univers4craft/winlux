# #2878 : au lancement de chaque .exe, les signatures (installateur Inno,
# SDL/GLFW lié statiquement) sont cherchées par memchr et non plus octet par
# octet, et le fichier n'est parcouru qu'une fois. Comportement mesuré par
# les tests unitaires search::tests (même résultat que la recherche naïve,
# 32 Mio en moins de 0,5 s en build de test), lancés par la CI.
_search_2878 = (ROOT / "runtime/src/search.rs").read_text(encoding="utf-8")
ok("pub fn find(hay: &[u8], needle: &[u8]) -> Option<usize>" in _search_2878
   and "fn meme_resultat_que_la_comparaison_octet_par_octet()" in _search_2878
   and "fn recherche_de_32_mio_rapide()" in _search_2878,
   "Lancement d'un .exe : signatures cherchées par memchr, mesure de 32 Mio en test (#2878)")
