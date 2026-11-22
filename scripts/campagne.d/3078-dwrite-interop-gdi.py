# Audacity (Qt) parcourt la collection système de DirectWrite pour bâtir sa liste
# de familles : Segoe UI (la police de NONCLIENTMETRICS) n'y figurait pas, et toute
# l'interface prenait une police de machine à écrire de xscreensaver (#3078).
# Qt demande aussi IDWriteGdiInterop et IDWriteFontFace5::GetFontResource.
_dw = (ROOT / "runtime" / "src" / "dlls" / "dwrite.rs").read_text(encoding="utf-8")
_gi = (ROOT / "runtime" / "src" / "dlls" / "dwrite_gdi.rs").read_text(encoding="utf-8")
print("GetGdiInterop servi :", "gdi_interop::get_gdi_interop" in _dw,
      "| GetFontResource servi :", "gdi_interop::face_resource" in _dw)
ok('unsupported!("IDWriteFontFace5::GetFontResource")' not in _dw
   and "fn logfont_segoe_ui_servi_et_nom_inconnu_refuse_3078" in _gi
   and "fn ressource_de_face_sans_axe_3078" in _gi,
   "DirectWrite sert IDWriteGdiInterop et IDWriteFontResource (#3078)")
ok("fn with_windows_families" in _dw and "fn collection_systeme_enumere_segoe_ui_3078" in _dw,
   "la collection système de DirectWrite énumère Segoe UI et les autres polices Windows remplacées (#3078)")
