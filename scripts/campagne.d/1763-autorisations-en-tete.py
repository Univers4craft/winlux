# #1763 : l'en-tête des autorisations des applications dit ce que fait le
# runtime : fichiers, réseau et lancement sans isolation accordés seuls,
# caméra, micro, capture d'écran et saisie demandés (Capability::asks).
settings_py = (SCRIPTS / "winlux-settings.py").read_text(encoding="utf-8")
permissions_rs = (ROOT / "runtime/src/permissions.rs").read_text(encoding="utf-8")
ok("Chaque application demande la première fois" not in settings_py
   and "sont accordés automatiquement ; caméra, micro, capture de l’écran et saisie à votre place sont demandés" in settings_py
   and "Capability::Camera | Capability::Microphone | Capability::Screen | Capability::Input" in permissions_rs,
   "Autorisations des applications : l’en-tête dit ce qui est accordé seul et ce qui est demandé (#1763)")
