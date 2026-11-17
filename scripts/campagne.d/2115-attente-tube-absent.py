# WaitNamedPipe rend tout de suite ERROR_FILE_NOT_FOUND sur un tube qui
# n'existe nulle part, ERROR_SEM_TIMEOUT sur un tube occupé, et le délai 0
# prend celui de CreateNamedPipe ; les instances des autres processus se
# voient par leur marqueur d'existence (#2115).
_s = (ROOT / "runtime/src/dlls/named_pipes.rs").read_text()
ok("fn attente_d_un_tube_absent_immediate_2115" in _s,
   "test de WaitNamedPipe sur un tube absent (#2115)")
ok("fn attente_d_un_tube_occupe_2115" in _s,
   "test de WaitNamedPipe sur un tube occupé (#2115)")
ok("fn tube_d_un_autre_processus_2115" in _s,
   "test du marqueur d'existence entre processus (#2115)")
