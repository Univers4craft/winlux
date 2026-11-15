# #2545 : DuplicateHandle d'une station de fenêtres ou d'un bureau vers un autre
# processus. Le comportement est gardé par le test unitaire nommé.
t = (runtime_dlls / "electron_win32.rs").read_text(encoding="utf-8")
ok("fn station_et_bureau_transmissibles_a_un_autre_processus" in t, "station et bureau transmis à un autre processus (#2545)")
