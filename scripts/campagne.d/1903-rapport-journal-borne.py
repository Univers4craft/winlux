# winlux-report ne lit que la fin du journal (#1903) et le journal du compte dans /run tourne au-delà de 32 Mo (#1904).
_r = (ROOT / "scripts/winlux-report.py").read_text(encoding="utf-8")
_l = (ROOT / "runtime/src/log.rs").read_text(encoding="utf-8")
ok("def tail_text(path, limit=1024 * 1024):" in _r
   and 'lines += tail_text(path).splitlines()[-limit:]' in _r
   and '"winlux.log").read_text' not in _r,
   "winlux-report lit la fin du journal, jamais tout le fichier (#1903)")
ok("const MAX_JOURNAL_SIZE: u64 = 32 * 1024 * 1024;" in _l
   and "rotate_journal(&mut guard, &path, MAX_JOURNAL_SIZE)" in _l
   and "fn journal_tourne_au_dela_de_la_limite" in _l,
   "journal du compte borné, rotation en winlux.log.old, avec test (#1904)")
