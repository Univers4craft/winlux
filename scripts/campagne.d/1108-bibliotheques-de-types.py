# Bibliothèques de types (#1108) : CreateTypeLib2, RegisterTypeLib, LoadRegTypeLib, LoadTypeLib, Invoke.
ok("fn create_type_lib2(" in (runtime_dlls / "oleaut32_typelib.rs").read_text()
   and "typelib::load_registered" in (runtime_dlls / "oleaut32.rs").read_text()
   and "bibliotheque_de_types_creee_inscrite_et_appelee" in (SCRIPTS / "test-all.sh").read_text(),
   "Bibliothèques de types : CreateTypeLib2, RegisterTypeLib, LoadRegTypeLib, LoadTypeLib, Invoke (#1108)")
