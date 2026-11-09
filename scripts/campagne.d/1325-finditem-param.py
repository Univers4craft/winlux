# LVM_FINDITEM avec LVFI_PARAM seul ne lit pas psz : WinSCP x86 y laisse 1
# et plantait avant la boîte Login (#1325).
ok("fn liste_recherche_par_parametre_sans_texte" in (ROOT / "runtime/src/dlls/controls/tests.rs").read_text(encoding="utf-8")
   and "flags & 0xE != 0" in (ROOT / "runtime/src/dlls/controls/comctl.rs").read_text(encoding="utf-8"),
   "LVM_FINDITEM par lParam sans lire le texte (#1325)")
