# AddFontMemResourceEx / RemoveFontMemResourceEx : police en mémoire dans
# un fichier privé du processus (SDL2_ttf, #1334).
ok("\"AddFontMemResourceEx\" => fn add_font_mem_resource_ex" in (ROOT / "runtime/src/dlls/gdi32_fonts.rs").read_text(encoding="utf-8")
   and "fn police_en_memoire_ajoutee_puis_retiree" in (ROOT / "runtime/src/dlls/gdi32_fonts.rs").read_text(encoding="utf-8"),
   "Polices en mémoire (#1334)")
