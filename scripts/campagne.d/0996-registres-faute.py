# #996 : registres et code autour de rip au journal de trace d'une faute matérielle.
ok("fn describe_registers" in (runtime_dlls / "seh.rs").read_text(encoding="utf-8")
   and "registres_d_une_faute_pour_le_journal" in (SCRIPTS / "test-all.d" / "0996-registres-faute.sh").read_text(encoding="utf-8"),
   "Faute matérielle : registres et octets de code au journal de trace (#996)")
