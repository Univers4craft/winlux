# Acrobat (#1818) : PathAppendW borné et canonique (#2042), CSIDL documentés (#2043).
_g = (ROOT / "runtime/src/dlls/games_win32.rs").read_text()
_k = (ROOT / "runtime/src/dlls/knownfolders.rs").read_text()
_p = _g[_g.index("fn PathAppendW"):]
_p = _p[:_p.index("\n}\n")]
ok("path_combine_units" in _p and "joined.len() >= 260" in _p and "take(259)" not in _p,
   "PathAppendW : canonique, FALSE au-delà de MAX_PATH (#2042)")
ok(all(f"0x{c} =>" in _k for c in ("13", "1b", "1f", "2d", "30", "38", "3b")),
   "SHGetFolderPath : CSIDL documentés servis (#2043)")
ok("SHGetFolderPathW(csidl" in _k and "PathAppendW({:?}" in _g,
   "SHGetFolderPathW et PathAppendW : lignes de trace (#2042, #2043)")
