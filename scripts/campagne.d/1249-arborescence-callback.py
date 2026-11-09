# SysTreeView32 demande au parent le texte d'un élément LPSTR_TEXTCALLBACK
# par TVN_GETDISPINFO (« New Site » vide dans WinSCP, #1249).
ok("fn tree_text" in (ROOT / "runtime/src/dlls/controls/comctl.rs").read_text(encoding="utf-8")
   and "fn arborescence_texte_demande_au_parent" in (ROOT / "runtime/src/dlls/controls/tests.rs").read_text(encoding="utf-8"),
   "Arborescence : texte LPSTR_TEXTCALLBACK demandé par TVN_GETDISPINFO (#1249)")
