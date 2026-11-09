# #1377 : champ « Page » de Sumatra, marges du champ et ordre des enfants.
controls = (runtime_dlls / "controls.rs").read_text(encoding="utf-8")
user32 = (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
ok("fn edit_margins" in controls, "Champ EDIT : marges EM_SETMARGINS et de la police (#1377)")
ok("fn restack_child" in user32, "SetWindowPos : ordre des fenêtres enfants (#1377)")
