# #2551 : user32!GetGUIThreadInfo est servie (GUITHREADINFO 64 bits).
# Le comportement est gardé par le test unitaire nommé.
t = (runtime_dlls / "user32_releve.rs").read_text(encoding="utf-8")
ok("fn infos_du_fil_graphique" in t, "GetGUIThreadInfo servie par user32 (#2551)")
