# #1768 à #1771 : polices de Windows remplacées par des polices libres.
fonts_rs = (runtime_dlls / "gdi32_fonts.rs").read_text(encoding="utf-8")
dwrite_rs = (runtime_dlls / "dwrite.rs").read_text(encoding="utf-8")
paquets = (ROOT / "scripts" / "prepare-desktop-packages.py").read_text(encoding="utf-8")
ok("fn enum_faces" in fonts_rs and '"MS Shell Dlg 2"' not in fonts_rs and '| "ms shell dlg 2"' in dwrite_rs,
   "Polices : MS Shell Dlg et Calibri Light comptent comme polices Windows remplacées (#1768)")
ok("forget_fonts()" in fonts_rs, "Polices : un ajout de police oublie les résolutions faites (#1769)")
ok('"Carlito"' in dwrite_rs and '"Caladea"' in dwrite_rs
   and "fonts-crosextra-carlito" in paquets and "fonts-crosextra-caladea" in paquets,
   "Polices : Calibri et Cambria remplacées par Carlito et Caladea (#1770)")
ok("FR_NOT_ENUM" in fonts_rs and '"GetGlyphIndicesA"' in fonts_rs and '"CreateFontIndirectExW"' in fonts_rs,
   "Polices : FR_NOT_ENUM, GetGlyphIndicesA et CreateFontIndirectEx (#1771)")
ok((ROOT / "docs" / "POLICES-WINDOWS.md").is_file(), "Polices de Windows : inventaire et licences (#1768)")
ok("(3, 0 | 1 | 10)" in fonts_rs, "Polices : noms en codage symbole lus, Marlett de Steam (#1807)")
