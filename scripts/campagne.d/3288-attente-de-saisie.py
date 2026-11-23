# #3288 : WaitForInputIdle attend qu'un autre processus ait une fenêtre, rend
# WAIT_TIMEOUT au délai et WAIT_FAILED s'il se termine sans fenêtre, au lieu
# de rendre 0 tout de suite. Gardé par le test unitaire nommé.
t = (runtime_dlls / "user32_more.rs").read_text(encoding="utf-8")
ok("fn attente_de_saisie_regarde_le_processus_3288(" in t, "WaitForInputIdle regarde le processus attendu (#3288)")
