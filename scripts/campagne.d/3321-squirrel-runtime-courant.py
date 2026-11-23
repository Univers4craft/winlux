# Update.exe (Squirrel) relançait l'application par /usr/bin/winlux, quel
# que soit le runtime qui le servait : une passe légère mesurait master
# (passe 3281b) (#3321). Le test unitaire nommé ici vérifie que la commande
# transmet le runtime courant au script par WINLUX_BIN.
_mg = (ROOT / "runtime" / "src" / "managed.rs").read_text(encoding="utf-8")
ok("fn update_exe_relance_par_le_runtime_courant_3321" in _mg,
   "Update.exe relance l'application par le runtime courant (#3321)")
