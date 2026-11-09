# DrawText coupe aux retours à la ligne et, avec DT_WORDBREAK, aux mots :
# la boîte « Information » de WinSCP affichait des carrés (#1341).
ok("fn draw_text_lines" in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
   and "fn draw_text_coupe_les_lignes_et_les_mots" in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8"),
   "DrawText multi-ligne et DT_WORDBREAK (#1341)")
