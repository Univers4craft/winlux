# Processus étranger fini mais non réclamé (zombie) : son handle est signalé (#1914).
src = (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
ok("fn os_process_zombie(" in src and "!os_process_alive(pid) || os_process_zombie(pid)" in src,
   "attente d'un processus ouvert : un zombie compte comme terminé (#1914)")
ok("fn processus_etranger_fini_mais_non_reclame_est_signale()" in src,
   "test : processus étranger zombie signalé (#1914)")
