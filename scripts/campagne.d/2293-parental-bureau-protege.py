# #2293 : Explorateur, Boutique, Gestionnaire des tâches et Bloc-notes PE
# tournent tous comme gui_demo.exe ; le bloquer fermait le bureau d'un compte surveillé.
par = (SCRIPTS / "winlux-parental.py").read_text(encoding="utf-8")
ok('SHELL_EXE = "gui_demo.exe"' in par and "def shell_process(" in par and "shell_process(pid, exe, cmdline)" in par,
   "contrôle parental : le vrai gui_demo.exe du bureau n'est jamais fermé (#2293)")
ok("protected(app[\"id\"].lower())" in par,
   "contrôle parental : une ancienne règle exe:gui_demo.exe est retirée sans perdre les autres (#2293)")
