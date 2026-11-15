# Le test OutputDebugString ne vérifie que ses propres lignes dans la file
# commune, et la file bornée sur une file locale (#2666).
_s = (ROOT / "runtime/src/dlls/kernel32_misc.rs").read_text()
_t = _s[_s.index("fn output_debug_string_garde_les_derniers_messages"):]
_t = _t[:_t.index("\n    }\n")]
ok("is_empty()" not in _t and "push_tail(&mut locale" in _t,
   "test OutputDebugString sur ses propres lignes (#2666)")
