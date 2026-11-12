# CopyFile garde la date de dernière écriture de la source, comme sous Windows (#1949).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
_s = (ROOT / "runtime/src/dlls/advapi32_services.rs").read_text(encoding="utf-8")
ok(_k.count("std::fs::copy(") == 1
   and _k.count("copy_keeping_write_time(&source, &target)") == 2
   and "fn copie_garde_la_date_de_derniere_ecriture" in _k
   and 'crate::trace!("StartServiceCtrlDispatcher(service' in _s,
   "CopyFile conserve la date de dernière écriture ; StartServiceCtrlDispatcher tracé (#1949)")
