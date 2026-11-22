# .NET : tableaux à plusieurs dimensions (T[,], CreateInstance, GetValue et
# SetValue à plusieurs indices) ; le témoin TableauxMultiples rend 200 sous
# scripts/test-bcl.sh (#2848).
_m = (ROOT / "runtime/src/clr/metadata.rs").read_text(encoding="utf-8")
_t = (ROOT / "runtime/src/clr/tableaux.rs").read_text(encoding="utf-8")
ok("Array(Box<ElementType>, u32)" in _m and "fn signature_array_lit_sa_forme" in _m
   and "fn md_array_access" in _t and "fn md_array_new" in _t
   and (ROOT / "bcl/temoins/TableauxMultiples.cs").exists(),
   ".NET : tableaux à plusieurs dimensions servis par le moteur, témoin TableauxMultiples (#2848)")
