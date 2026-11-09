# #1364 : BCryptHash accepte les pseudo-handles d'algorithme CNG.
ok("fn resolve(alg: usize)" in (runtime_dlls / "bcrypt.rs").read_text(encoding="utf-8"),
   "BCrypt : pseudo-handles d'algorithme (#1364)")
