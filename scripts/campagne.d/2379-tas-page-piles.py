# #2379 : en mode page, la faute donne les piles d'allocation et de
# libération du bloc. Le comportement est gardé par le test nommé ci-dessous.
tp = (ROOT / "runtime" / "src" / "dlls" / "tas_page.rs").read_text(encoding="utf-8")
cr = (ROOT / "runtime" / "src" / "crash.rs").read_text(encoding="utf-8")
ok("fn scan_stack" in tp, "le mode page relève la pile (#2379)")
ok("fn write_heap_frames" in cr, "la faute écrit les piles du bloc (#2379)")
ok("fn tas_page_nomme_le_bloc_a_la_faute" in cr, "test des piles à la faute (#2379)")
