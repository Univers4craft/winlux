# MoveFileTransactedW et DeleteFileTransactedW pour l'installeur de Steam x86 (#1475).
ok("MoveFileTransactedW" in (ROOT / "docs/compat/kernel32-surface.txt").read_text(encoding="utf-8")
   and "DeleteFileTransactedW" in (ROOT / "docs/compat/kernel32-surface.txt").read_text(encoding="utf-8")
   and "(#1475)" in (ROOT / "testc/wow32/crt32.c").read_text(encoding="utf-8"),
   "déplacements transactionnels de Steam faits directement (#1475)")
