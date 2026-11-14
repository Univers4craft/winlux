# #2320 : Component.Dispose() et Container.Dispose() appellent
# GC.SuppressFinalize(this) après Dispose(true), comme .NET.
src = (ROOT / "bcl" / "mscorlib" / "Composants.cs").read_text(encoding="utf-8")
ok(src.count("Dispose(true);\n            // Libéré explicitement : plus de finalisation (#2320).\n            GC.SuppressFinalize(this);") == 2,
   "CLR : Component et Container suppriment leur finalisation après Dispose (#2320)")
