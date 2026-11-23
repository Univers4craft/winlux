# #3405 : avec 7-Zip installé, un 7z qui contient un lien vers /etc ou
# « ../../.. » échouait en entier (code 2 de 7-Zip, qui refuse lui-même ces
# liens) et cassait les releases. Ces refus seuls ne sont plus un échec ; une
# vraie erreur est montrée par sa ligne de stderr, pas par la progression.
# Comportement gardé par le test unitaire nommé.
_tools_3405 = (ROOT / "runtime/src/archive/tools.rs").read_text(encoding="utf-8")
ok("fn liens_refuses_par_7zip_ne_sont_pas_un_echec_3405" in _tools_3405
   and "fn only_refused_links(" in _tools_3405,
   "liens refusés par 7-Zip : l'archive s'extrait sans eux (#3405)")
