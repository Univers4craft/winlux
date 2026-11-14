# #2050 : System.Diagnostics.Process dans notre mscorlib, au-dessus de
# kernel32 ; un zombie est terminé (#1914) et le processus d'un autre compte
# ne s'ouvre pas.
bcl = ROOT / "bcl" / "mscorlib"
clr = runtime_dlls.parent / "clr"
ok("public static Process GetProcessById(int processId)" in (bcl / "Processus.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Processus.cs").read_text(encoding="utf-8"),
   "CLR : System.Diagnostics.Process dans notre mscorlib (#2050)")
ok("fn same_account" in (clr / "processus.rs").read_text(encoding="utf-8")
   and "os_process_zombie" in (clr / "processus.rs").read_text(encoding="utf-8"),
   "CLR : Process n'ouvre pas un autre compte et tient un zombie pour fini (#2050)")
# #2071 : le pid 0 est le processus « Idle », comme sous Windows.
ok('if (processId == Inactif) return new Process(Inactif, "Idle");' in (bcl / "Processus.cs").read_text(encoding="utf-8"),
   "CLR : Process.GetProcessById(0) rend le processus Idle (#2071)")
