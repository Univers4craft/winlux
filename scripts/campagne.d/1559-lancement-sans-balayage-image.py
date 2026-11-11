# Le lancement ne balaie plus toute l'image du .exe : seulement .rdata/.data, en un passage (#1559).
ok("renderer_in_data_sections(image.bytes(), &image.pe.sections)" in (ROOT / "runtime/src/main.rs").read_text(encoding="utf-8")
   and ".windows(needle.len())" not in (ROOT / "runtime/src/dlls/gui_x11.rs").read_text(encoding="utf-8"),
   "classe SDL/GLFW cherchée dans les seules sections de données, en un passage (#1559)")
