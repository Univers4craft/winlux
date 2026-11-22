# CRT : _mbspbrk et ses sœurs _mbschr, _mbsnbcmp, _mbsnbcpy pour OBS Studio (#2218).
_m = (ROOT / "runtime/src/dlls/mod.rs").read_text()
_t = (ROOT / "runtime/src/dlls/crt_mbs/tests.rs").read_text()
ok("pub mod crt_mbs;" in _m and "crt_mbs::exports" in _m and "fn exports_de_la_famille(" in _t,
   "ucrtbase et msvcrt servent les fonctions multioctets de crt_mbs (#2218)")
_p = (ROOT / "runtime/src/dlls/crt_mbs.rs").read_text()
ok(all(f'"{n}" => fn' in _p for n in ("_mbspbrk", "_mbschr", "_mbsnbcmp", "_mbsnbcpy")),
   "_mbspbrk, _mbschr, _mbsnbcmp et _mbsnbcpy exportées (#2218)")
