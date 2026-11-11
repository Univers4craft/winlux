# #1727 : la base d'image d'une exception C++ ne doit pas dépendre d'un essai
# sans attente sur la table des modules (une DLL chargée en parallèle la vidait).
seh_src = (runtime_dlls / "seh.rs").read_text(encoding="utf-8")
debut = seh_src.index("pub fn module_base(")
corps = seh_src[debut:seh_src.index("\n}\n", debut)]
ok("try_lock" not in corps and "crate::dlls::lock(&crate::loader::MODULES)" in corps,
   "exception C++ : base d'image trouvée même pendant un chargement de DLL (#1727)")
