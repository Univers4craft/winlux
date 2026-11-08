# DrawText retire « & » et souligne la lettre de raccourci ; DrawTextEx suit
# le même rendu (libellés « &Hostname: » de WinSCP, #1248).
ok("fn split_prefix" in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
   and '"DrawTextExW"=>fn draw_text_ex_w' in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
   and "DrawTextW retire « & » sauf DT_NOPREFIX" in (ROOT / "testc/wow32/dlg32.c").read_text(encoding="utf-8"),
   "DrawText : préfixe de mnémonique traité, DrawTextEx réel (#1248)")
