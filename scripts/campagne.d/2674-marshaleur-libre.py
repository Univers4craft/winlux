# #2674 : CoCreateFreeThreadedMarshaler crée un vrai marshaleur agrégeable
# (IUnknown non nul, IMarshal) au lieu de rendre S_OK avec un pointeur nul.
f = (ROOT / "runtime/src/dlls/ole32_ftm.rs").read_text(encoding="utf-8")
ok("fn marshaleur_libre_cree_et_agrege_2674" in f,
   "COM : CoCreateFreeThreadedMarshaler rend un marshaleur (#2674)")
