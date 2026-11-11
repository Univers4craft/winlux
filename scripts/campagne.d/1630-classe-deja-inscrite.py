# #1630 : RegisterClassEx refuse une classe déjà inscrite par le même module (ERROR_CLASS_ALREADY_EXISTS).
u = (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
ok("ERROR_CLASS_ALREADY_EXISTS" in u and "classe_deja_inscrite_refusee_puis_reinscrite" in u,
   "RegisterClassEx rend 0 et 1410 pour une classe déjà inscrite, UnregisterClass la libère (#1630)")
