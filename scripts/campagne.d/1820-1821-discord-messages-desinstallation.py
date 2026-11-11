# #1820 : FormatMessage rend l'anglais quand LANG_ENGLISH est demandé (libuv).
# #1821 : Update.exe natif de Squirrel gère --uninstall.
m = (runtime_dlls / "kernel32_misc.rs").read_text(encoding="utf-8")
ok("fn system_message_english" in m and "wants_english(lang)" in m,
   "FormatMessage : texte anglais si LANG_ENGLISH est demandé (#1820)")
u = (SCRIPTS / "winlux-squirrel-update.py").read_text(encoding="utf-8")
ok('command == "uninstall"' in u and '"--squirrel-uninstall"' in u and '"--remove"' in u,
   "Update.exe --uninstall : crochet puis désinstallation Winlux (#1821)")
