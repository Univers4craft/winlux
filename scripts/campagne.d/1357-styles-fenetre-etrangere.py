# #1357 : GetWindowLong rend les styles publiés d'une fenêtre étrangère.
ok("pub ex_style: u32" in (runtime_dlls / "gui_shared.rs").read_text(encoding="utf-8")
   and "fn publish_shared" in (runtime_dlls / "user32.rs").read_text(encoding="utf-8"),
   "Styles des fenêtres d'un autre processus (#1357)")
