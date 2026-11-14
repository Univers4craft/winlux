# #2306 : List<T>.AsReadOnly dans notre mscorlib ; Corsair.Service.exe
# (iCUE) lancé à la main mourait sur MissingMethodException, code 82.
bcl = ROOT / "bcl" / "mscorlib"
ok("ReadOnlyCollection<T> AsReadOnly()" in (bcl / "Collections.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "LectureSeule.cs").read_text(encoding="utf-8"),
   "CLR : List<T>.AsReadOnly (#2306)")
