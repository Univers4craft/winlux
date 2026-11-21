# SuspendThread : jamais d'arrêt forcé d'un fil qui tourne dans Winlux ou la glibc (#2165).
_t = (ROOT / "runtime/src/dlls/thread_ctl.rs").read_text(encoding="utf-8")
ok("const FORCE_AFTER" not in _t, "plus d'arrêt forcé au bout de 200 ms hors du code du programme (#2165)")
# Test unitaire arret_rapide_d_un_thread_endormi : un fil endormi dans le noyau est arrêté.
ok("fn in_blocking_syscall(" in _t and "fn arret_rapide_d_un_thread_endormi(" in _t, "arrêt forcé seulement dans un appel système bloquant (#2165)")
ok("fn jamais_arrete_au_milieu_de_malloc" in _t, "test d'interblocage malloc/SuspendThread (#2165)")
