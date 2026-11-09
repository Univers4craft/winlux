# #1453 : l'autorisation donnée avant le téléchargement vaut jusqu'à
# l'installation, même après 5 minutes, par un accord à usage unique lié au
# compte, au logiciel et au SHA-256 ; il n'ouvre aucune autre commande apt.
root_py = (SCRIPTS / "winlux-root.py").read_text(encoding="utf-8")
repos_py = (SCRIPTS / "winlux-store-repos.py").read_text(encoding="utf-8")
ok("def store_grant" in root_py and "STORE_GRANT_SECONDS = 3600" in root_py and "def install(" in repos_py,
   "Boutique : un long téléchargement ne redemande pas l'autorisation (#1453)")
