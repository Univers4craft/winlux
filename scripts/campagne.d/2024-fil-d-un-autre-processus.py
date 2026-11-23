# OpenThread et ThreadBasicInformation sur le fil d'un autre processus du même compte (#2024).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
ok("fn open_thread_ouvre_le_fil_d_un_autre_processus" in _k,
   "OpenThread ouvre le fil d'un autre processus du même compte (#2024)")
ok((ROOT / "testexe/src/bin/fil_etranger.rs").exists()
   and "fil_etranger" in (ROOT / "scripts/test-runtime.sh").read_text(),
   "test PE fil_etranger : TEB d'un fil d'un autre processus lu par ReadProcessMemory (#2024)")
