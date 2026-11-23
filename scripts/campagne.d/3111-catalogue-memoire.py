# #3111 : la passe du catalogue tourne dans un processus fils ; le démon résident
# ne garde pas la mémoire de la relecture du registre.
_c_3111 = runpy.run_path(str(SCRIPTS / "winlux-applications.py"), run_name="catalogue_3111")
_journal_3111 = []
_c_3111["refresh_apart"].__globals__["refresh"] = lambda: _journal_3111.append(os.getpid())
_c_3111["refresh_apart"]()
print("passe du catalogue, pid du démon", os.getpid(), "pid de la passe :", _journal_3111 or "(dans le fils)")
ok(not _journal_3111, "catalogue : la passe refresh() tourne hors du démon résident, mémoire rendue à chaque passe (#3111)")
