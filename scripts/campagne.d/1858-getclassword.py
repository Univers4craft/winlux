# GetClassWord exporté, GetClassLong(Ptr) relit atome, procédure, module, fond et curseur de classe : Steam ne s'arrête plus en 0xC0000139 après le clic (#1858).
_u = (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
_e = (ROOT / "runtime/src/dlls/user32_ext.rs").read_text(encoding="utf-8")
ok('"GetClassWord"=>fn get_class_word' in _u and "pub fn class_value(hwnd: usize, index: i32)" in _u,
   "GetClassWord exporté, valeurs de classe relues (#1858)")
ok("fn valeurs_de_classe_relues" in _u and _e.count("crate::dlls::user32::class_value(hwnd, index)") == 4,
   "test, et les doublons de user32_ext délèguent (#1858)")
