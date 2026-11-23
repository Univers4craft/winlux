# #3411 : sur l'ISO (bsdtar), un 7z ou un RAR avec une seule entrée « ../ »
# ne s'extrayait plus du tout : bsdtar refuse l'entrée, extrait le reste et
# sort en code 1, que l'on prenait pour un échec total. Ces refus seuls ne
# sont plus un échec. Comportement gardé par le test unitaire nommé.
_tools_3411 = (ROOT / "runtime/src/archive/tools.rs").read_text(encoding="utf-8")
ok("fn entree_qui_remonte_refusee_sans_echec_total_3411" in _tools_3411
   and "Path contains '..'" in _tools_3411,
   "entrée « ../ » refusée par bsdtar : l'archive s'extrait sans elle (#3411)")
