# #2192 : Teams demande la fabrique Windows.Foundation.PropertyValue au démarrage.
p = (runtime_dlls / "property_value.rs").read_text(encoding="utf-8")
m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
ok("pub mod property_value;" in m
   and 'const CLASS: &str = "Windows.Foundation.PropertyValue";' in p
   and "fn fabrique_et_valeurs(" in p,
   "WinRT : Windows.Foundation.PropertyValue a sa fabrique, testée (#2192)")
ok("fn reference_iid(" in p and "fn identifiants_d_ireference(" in p,
   "valeurs emballées lisibles par IReference<T>, identifiants vérifiés (#2192)")
