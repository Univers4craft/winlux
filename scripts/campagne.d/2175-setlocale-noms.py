# #2175 : Teams construit std::locale("en_US.UTF-8") ; setlocale le refusait.
c = (runtime_dlls / "crt_runtime.rs").read_text(encoding="utf-8")
ok("fn locale_name_ok(" in c and '"en_US.UTF-8"' in c,
   "setlocale accepte les noms de locale documentés, dont en_US.UTF-8 (#2175)")
ok("LC_COLLATE" in c and "fn locale_apply(" in c,
   "setlocale retient le nom de chaque catégorie, forme composée pour LC_ALL (#2175)")
