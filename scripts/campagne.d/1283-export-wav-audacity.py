# _get_osfhandle rend un vrai handle pour un fichier ouvert par le CRT :
# l'export WAV d'Audacity écrit son contenu.
ok("fn forget_osfhandle" in (runtime_dlls / "crt_runtime.rs").read_text()
   and "get_osfhandle_d_un_fichier" in (SCRIPTS / "test-all.d" / "1283-get-osfhandle.sh").read_text(),
   "_get_osfhandle d'un fichier CRT : export WAV d'Audacity non vide (#1283)")
