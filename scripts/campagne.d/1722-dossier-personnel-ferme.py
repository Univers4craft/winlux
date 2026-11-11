# #1722 : un dossier personnel n'est jamais inscriptible par le groupe ni par
# les autres. L'init le referme à chaque démarrage (systèmes déjà touchés par
# l'installeur fautif), et l'invité natif le vérifie après le démarrage.
iso_init = (SCRIPTS / "build-live-iso.sh").read_text(encoding="utf-8")
guest = (SCRIPTS / "test-apt-native-guest.py").read_text(encoding="utf-8")
ok("$((0$home_mode & 022))" in iso_init and '/bin/busybox chmod 700 "$home_dir"' in iso_init
   and "#1722" in guest and "mode & 0o022" in guest,
   "Dossier personnel jamais ouvert en écriture aux autres, refermé au démarrage (#1722)")
