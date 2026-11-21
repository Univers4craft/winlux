# #2219 : ConnectNamedPipe asynchrone inscrit son opération comme ReadFile :
# hEvent remis à non signalé au départ, GetOverlappedResult(bWait = TRUE)
# attend la connexion. Teams s'arrêtait sur « GetOverlappedResult for pipe
# connection failed ».
pipes = (ROOT / "runtime" / "src" / "dlls" / "named_pipes.rs").read_text(encoding="utf-8")
k32 = (ROOT / "runtime" / "src" / "dlls" / "kernel32.rs").read_text(encoding="utf-8")
# Comportement vérifié par le test unitaire connexion_asynchrone_attendue_2219.
ok("kernel32::begin_overlapped" in pipes
   and "fn begin_overlapped(" in k32
   and "fn connexion_asynchrone_attendue_2219" in pipes,
   "ConnectNamedPipe asynchrone attendu par GetOverlappedResult (#2219)")
