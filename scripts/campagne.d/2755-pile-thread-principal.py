# #2755 : la pile du thread principal d'un programme Windows reçoit, en plus
# de la réserve de son en-tête PE, une marge pour les cadres de Winlux
# (Paint.NET débordait sa pile de 1 Mio). Comportement gardé par p1_suite.exe
# (GetCurrentThreadStackLimits) et le test process::pile_principale.
process_2755 = (ROOT / "runtime" / "src" / "process.rs").read_text(encoding="utf-8")
p1_2755 = (ROOT / "testexe" / "src" / "bin" / "p1_suite.rs").read_text(encoding="utf-8")
ok("fn main_stack_size" in process_2755 and "pile du thread principal" in p1_2755,
   "pile du thread principal : en-tête plus 8 Mio de marge (#2755)")
