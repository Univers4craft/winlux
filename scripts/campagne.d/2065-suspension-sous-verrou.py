# SuspendThread n'arrête pas un thread qui tient un verrou de Winlux (#2065).
_t = (ROOT / "runtime/src/dlls/thread_ctl.rs").read_text(encoding="utf-8")
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
_a = (ROOT / "runtime/src/thunks.rs").read_text(encoding="utf-8")
ok("pub fn no_park()" in _t and "if held() ||" in _t, "garde NoPark respectée par le gestionnaire de suspension (#2065)")
ok("asleep(tid)" in _t and "FORCE_ASLEEP_AFTER" in _t, "arrêt forcé rapide seulement pour un thread endormi dans le noyau (#2065)")
ok("fn allocs() -> AllocsGuard" in _k and "lock(ALLOCS.get_or_init" not in _k.replace("maps: lock(ALLOCS.get_or_init", ""), "table VirtualAlloc toujours prise sous garde NoPark (#2065)")
ok("thread_ctl::no_park()" in _a, "journal d'API pris sous garde NoPark (#2065)")
ok("fn pas_d_arret_sous_garde_no_park" in _t and "fn arret_rapide_d_un_thread_endormi" in _t, "tests de la suspension sous verrou (#2065)")
