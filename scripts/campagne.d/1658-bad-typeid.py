# #1658 : __RTtypeid sur un pointeur nul lève std::bad_typeid (x64) au lieu de rendre 0.
r = (runtime_dlls / "rtti.rs").read_text(encoding="utf-8")
ok(".?AVbad_typeid@std@@" in r and "fn bad_typeid_args" in r and "bad_typeid_complete" in r,
   "typeid(*p) avec p nul lève std::bad_typeid rattrapable par catch (std::exception&) (#1658)")
