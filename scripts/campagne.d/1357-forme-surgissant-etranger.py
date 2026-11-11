# #1357 : un surgissant d'un autre processus peint avec alpha est découpé.
ok("fn shape_foreign_popup" in (runtime_dlls / "gdi32.rs").read_text(encoding="utf-8")
   and "fn shape_foreign" in (runtime_dlls / "gui_x11.rs").read_text(encoding="utf-8"),
   "Forme des surgissants d'un autre processus (#1357)")
