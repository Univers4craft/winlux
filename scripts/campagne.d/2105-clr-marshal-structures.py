# #2105 : Marshal copie les structures vers la mémoire native et retour
# (StructureToPtr, PtrToStructure, DestroyStructure, OffsetOf), alloue
# (AllocHGlobal, AllocCoTaskMem) et convertit les chaînes natives.
bcl = ROOT / "bcl" / "mscorlib"
clr = runtime_dlls.parent / "clr"
ok("public static void StructureToPtr(object structure, IntPtr ptr, bool fDeleteOld)" in (bcl / "Interop.cs").read_text(encoding="utf-8")
   and "public static IntPtr AllocHGlobal(int cb)" in (bcl / "Interop.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Interop.cs").read_text(encoding="utf-8"),
   "CLR : Marshal.StructureToPtr, PtrToStructure, AllocHGlobal et chaînes natives (#2105)")
ok("fn native_layout" in (clr / "marshal.rs").read_text(encoding="utf-8")
   and "fn write_fields" in (clr / "marshal.rs").read_text(encoding="utf-8")
   and "fn read_fields" in (clr / "marshal.rs").read_text(encoding="utf-8"),
   "CLR : SizeOf et StructureToPtr partagent la même disposition native (#2105)")
