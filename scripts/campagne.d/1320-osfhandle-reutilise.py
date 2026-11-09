crt = (ROOT / "runtime/src/dlls/crt_runtime.rs").read_text(encoding="utf-8")
ok("fn same_file(" in crt and "Some(copy) if same_file(fd, copy)" in crt,
   "_get_osfhandle ne rend pas le handle d'un descripteur réutilisé pour un autre fichier (#1320)")
ok("fn get_osfhandle_suit_un_numero_reutilise" in crt, "test du descripteur réutilisé (#1320)")
