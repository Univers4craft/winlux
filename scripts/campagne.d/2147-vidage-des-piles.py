# WINLUX_THREAD_DUMP : vidage des piles sur SIGUSR2, rien sans la variable (#2147).
_t = (ROOT / "runtime/src/dlls/thread_ctl.rs").read_text(encoding="utf-8")
_m = (ROOT / "runtime/src/main.rs").read_text(encoding="utf-8")
ok("fn install_dump_from(" in _t and "libc::SIGUSR2" in _t, "vidage des piles sur SIGUSR2 (#2147)")
ok(_t.count("WINLUX_THREAD_DUMP\")") == 1, "variable WINLUX_THREAD_DUMP lue une seule fois (#2147)")
ok("dlls::thread_ctl::install_dump();" in _m, "vidage installé au démarrage du processus (#2147)")
ok("fn vidage_absent_sans_la_variable" in _t and "fn vidage_d_un_thread_qui_tourne" in _t, "tests du vidage des piles (#2147)")
ok("fn read_own_memory(" in _t and "process_vm_readv" in _t and "stack_top" in _t, "pile lue par process_vm_readv, bornée par le sommet connu de la pile (#2147)")
ok("fn vidage_de_plusieurs_threads_sans_dommage" in _t and "fn lecture_bornee_par_une_page_illisible" in _t, "le vidage ne fait pas planter le processus observé (#2147)")
