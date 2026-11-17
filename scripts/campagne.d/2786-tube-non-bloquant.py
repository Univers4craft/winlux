# #2786 : un tube nommé en PIPE_NOWAIT (CreateNamedPipe ou
# SetNamedPipeHandleState) ne bloque pas ReadFile : un tube vide rend
# ERROR_NO_DATA (installeur de Creative Cloud). Comportement gardé par le
# test dlls::named_pipes::tube_non_bloquant_2786.
tubes_2786 = (ROOT / "runtime" / "src" / "dlls" / "named_pipes.rs").read_text(encoding="utf-8")
kernel32_2786 = (ROOT / "runtime" / "src" / "dlls" / "kernel32.rs").read_text(encoding="utf-8")
ok("fn tube_non_bloquant_2786" in tubes_2786
   and "fn is_nowait_pipe" in kernel32_2786,
   "Tube nommé PIPE_NOWAIT : ReadFile rend ERROR_NO_DATA sans bloquer (#2786)")
