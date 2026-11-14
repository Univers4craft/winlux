# #2158 : WINLUX_HEAP_DIAG=page sert chaque bloc HeapAlloc sur ses pages,
# collé à une page de garde, et le gestionnaire de fautes nomme le bloc
# touché. Le comportement est gardé par les tests unitaires nommés ci-dessous.
tp = (ROOT / "runtime" / "src" / "dlls" / "tas_page.rs").read_text(encoding="utf-8")
cr = (ROOT / "runtime" / "src" / "crash.rs").read_text(encoding="utf-8")
ok("fn bloc_page_colle_a_sa_garde_et_retrouve_apres_liberation" in tp, "test du bloc page et de sa garde (#2158)")
ok("fn tas_page_nomme_le_bloc_a_la_faute" in cr, "test de la faute qui nomme le bloc (#2158)")
ok("fn write_heap_page" in cr, "le gestionnaire de fautes écrit le bloc de tas (#2158)")
