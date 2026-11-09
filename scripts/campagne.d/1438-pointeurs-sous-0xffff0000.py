# Les 64 derniers Ko sous 4 Go restent réservés, comme sous WOW64 (#1438).
ok("reserve(0xffff_0000, limit)" in (ROOT / "runtime/src/wow32/mod.rs").read_text(encoding="utf-8"),
   "aucune allocation x86 au-dessus de 0xffff0000 (#1438)")
