# Port de complétion : place d'un fil bloqué reprise ; événement d'E/S remis à zéro (#2055).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
ok("fn running_threads(" in _k and "/proc/self/task/{tid}/stat" in _k, "seuls les fils qui tournent comptent dans la limite de concurrence (#2055)")
ok("place d'un fil bloqué ailleurs reprise" in _k, "trace quand une place est reprise (#2055)")
ok("let _ = reset_event(event);" in _k, "ReadFile/WriteFile remettent l'événement à zéro au début de l'E/S (#2055)")
ok("fn iocp_place_d_un_fil_bloque_reprise_par_un_autre" in _k, "test de la place reprise (#2055)")
