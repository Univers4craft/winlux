# Un processus qui plante ferme ses jobs KILL_ON_JOB_CLOSE, comme sous Windows (#1966).
_c = (ROOT / "runtime/src/crash.rs").read_text(encoding="utf-8")
_j = (ROOT / "runtime/src/dlls/jobs.rs").read_text(encoding="utf-8")
ok("crate::dlls::jobs::on_crash();" in _c
   and "fn plantage_tue_les_membres_kill_on_close" in _j,
   "plantage : les membres des jobs KILL_ON_JOB_CLOSE sont tués (#1966)")
