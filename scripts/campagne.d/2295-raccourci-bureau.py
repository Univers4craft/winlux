# Assistant : la case « Créer un raccourci sur le Bureau » est respectée pour NSIS, dans les deux sens (#2295).
_s = (ROOT / "runtime/src/setup/mod.rs").read_text(encoding="utf-8")
ok("pub fn ensure_desktop(" in _s and "scripts/test-raccourci-bureau.sh" in (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
   "assistant : raccourci du Bureau posé si la case est cochée, jamais si elle est décochée (#2295)")
