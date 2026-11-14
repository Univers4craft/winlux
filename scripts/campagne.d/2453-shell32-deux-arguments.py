# shell32 : SHCreateShellFolderView et SHCreateDefaultExtractIcon mettent à zéro leur deuxième argument, pas un troisième (#2453).
_s = (ROOT / "runtime/src/dlls/shell32_releve.rs").read_text()
ok("fn refus_hors_processus_met_la_bonne_sortie_a_zero" in _s,
   "la vue de dossier et l'extracteur d'icônes par défaut remettent leur vraie sortie à NULL (#2453)")
