# winlux-report ne lit que la fin du journal (#1903) et le journal du compte dans /run tourne au-delà de 32 Mo (#1904).
_r = (ROOT / "scripts/winlux-report.py").read_text(encoding="utf-8")
_l = (ROOT / "runtime/src/log.rs").read_text(encoding="utf-8")
# Comportement vérifié par scripts/test-report.py (journal de plusieurs Go :
# seule la fin est lue, #1903) ; ici, les noms.
ok("def tail_text(" in _r
   and "(#1903)" in (ROOT / "scripts/test-report.py").read_text(encoding="utf-8"),
   "winlux-report lit la fin du journal, jamais tout le fichier, avec test (#1903)")
ok("const MAX_JOURNAL_SIZE: u64 = 32 * 1024 * 1024;" in _l
   and "fn rotate_journal(" in _l
   and "fn journal_tourne_au_dela_de_la_limite" in _l,
   "journal du compte borné, rotation en winlux.log.old, avec test (#1904)")
