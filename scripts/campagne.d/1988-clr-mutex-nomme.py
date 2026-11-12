# #1988 : Mutex et Semaphore nommés de notre mscorlib, adossés aux objets
# nommés de kernel32 (partagés entre processus du compte, #20), avec
# MutexSecurity, MutexAccessRule et SecurityIdentifier ; témoin Synchro.
bcl_synchro = (ROOT / "bcl" / "mscorlib" / "Synchro.cs").read_text(encoding="utf-8")
ok("public sealed class Mutex : WaitHandle" in bcl_synchro and "public sealed class Semaphore : WaitHandle" in bcl_synchro
   and "public sealed class MutexSecurity" in bcl_synchro
   and "fn clr_mutex_create" in (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Synchro.cs").read_text(encoding="utf-8"),
   "CLR : Mutex et Semaphore nommés sur les objets de kernel32, témoin Synchro (#1988)")
