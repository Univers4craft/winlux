# #2648 : pendant l'appel d'un gestionnaire SEH x86, la tête de fs:[0] est
# un cadre du répartiteur (la CRT de MSVC la relève et la remet en place ;
# sans lui, l'outil de déploiement d'Office gardait un cadre mort en tête).
s = (ROOT / "runtime/src/wow32/seh.rs").read_text(encoding="utf-8")
c = (ROOT / "testc/wow32/seh32.c").read_text(encoding="utf-8")
ok("fn call_handler" in s and "struct NestedFrame" in s and "nested_probe" in c,
   "SEH x86 : cadre du répartiteur en tête pendant un gestionnaire (#2648)")
