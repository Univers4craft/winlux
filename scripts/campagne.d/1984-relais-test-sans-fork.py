# Test du relais Nt* : processus neuf au lieu d'un fork de l'exécuteur multithread (#1984).
_c = (ROOT / "runtime/src/dlls/pe_image.rs").read_text()
_t = _c[_c.index("fn relais_de_service_au_format_windows"):]
ok("libc::fork()" not in _t and "current_exe()" in _t and "WINLUX_TEST_RELAIS_ENFANT" in _t,
   "relais Nt* : enfant relancé par current_exe, plus de fork (#1984)")
ok("processus enfant bloqué plus de 60 s" in _t, "relais Nt* : attente de l'enfant bornée (#1984)")
