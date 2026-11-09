icons = (ROOT / "runtime/src/dlls/icons.rs").read_text(encoding="utf-8")
barres = (ROOT / "runtime/src/dlls/controls/bars.rs").read_text(encoding="utf-8")
ok("info.add(24) as *mut [i32; 4]" in icons, "ImageList_GetImageInfo écrit rcImage à l'offset 24 d'IMAGEINFO (#1287)")
ok("Un HBITMAP nul n'ajoute rien" in barres, "TB_ADDBITMAP avec un HBITMAP nul ne consomme aucun index d'image (#1287)")
