# #2530 : un enfant x86 lancé sur un tube (STARTF_USESTDHANDLES) qui faute
# n'écrit aucune ligne de Winlux dans ce tube (descripteur de diagnostic
# gardé à la relance wow32, plus d'écriture directe sur le descripteur 2).
l = (ROOT / "runtime/src/log.rs").read_text(encoding="utf-8")
t = (ROOT / "scripts/test-wow32.sh").read_text(encoding="utf-8")
ok("fn keep_diag_fd_across_exec" in l and "check tube32" in t,
   "diagnostic d'un enfant x86 hors du tube de son père (#2530)")
