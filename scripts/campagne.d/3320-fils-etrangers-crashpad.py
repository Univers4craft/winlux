# Crashpad garde les fils d'un autre processus : Suspend/Resume sans effet
# sur un processus arrêté, Rip et Rsp seuls, garde-fou refait à chaque
# appel (#3320).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
_pe = (ROOT / "testexe/src/bin/fil_etranger.rs").read_text(encoding="utf-8")
ok("fn fil_etranger_etat_starttime_et_registres_3320" in _k
   and "SuspendThread : 1 sur l'enfant suspendu" in _pe,
   "fil d'un autre processus : crashpad garde ses fils, sans pouvoir d'action (#3320)")
