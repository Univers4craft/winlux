# #1377, #1378, #1379, #1381 : barre d'outils mixte, pavé numérique,
# RoundRect sous NULL_BRUSH et chemin Windows passé au lanceur.
ok("fn shown_text" in (runtime_dlls / "controls" / "bars.rs").read_text(encoding="utf-8"),
   "Barre d'outils : texte des boutons sans BTNS_SHOWTEXT réservé à l'infobulle (#1377)")
ok("0xFFAB => 0x6B" in (runtime_dlls / "gui_x11.rs").read_text(encoding="utf-8"),
   "Clavier : « + » du pavé numérique traduit en VK_ADD (#1378)")
ok("fn round_rect_outline" in (runtime_dlls / "gdi32.rs").read_text(encoding="utf-8"),
   "GDI : RoundRect respecte NULL_BRUSH et trace le contour à la plume (#1379)")
ok("pub fn launch_path" in (ROOT / "runtime" / "src" / "paths.rs").read_text(encoding="utf-8"),
   "Lanceur : chemin Windows du programme traduit (#1381)")
