# Les polices Windows absentes (Tahoma, Arial…) sont énumérées sous leur nom, servies par leur remplaçante : vgui de Steam les trouve (#1670).
_f = (ROOT / "runtime/src/dlls/gdi32_fonts.rs").read_text(encoding="utf-8")
ok("pub fn enum_faces(face: &str)" in _f and _f.count("for f in enum_faces(face) {") == 2,
   "EnumFontFamilies(Ex) A et W passent par enum_faces (#1670)")
ok('"Tahoma"' in _f and '"Arial"' in _f and "fn polices_windows_enumerees_sous_leur_nom" in _f,
   "Tahoma et Arial énumérées sous leur nom, avec test (#1670)")
