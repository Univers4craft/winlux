# #1716 : Brave, Opera, Opera GX, Tor Browser et Vivaldi proposés au choix du
# navigateur (tous ont une version Linux officielle), partout où le choix se
# fait, dans l'ordre alphabétique, aucun par défaut ; Tor Browser vérifié par
# la signature de ses développeurs (clé épinglée) en plus du SHA-256.
import re as _re
nav_ids = ["brave", "chrome", "edge", "firefox", "opera", "operagx", "tor", "vivaldi"]
nav_alt = "|".join(nav_ids)
nav_fetch = (SCRIPTS / "prepare-browser-packages.py").read_text(encoding="utf-8")
nav_sh = (SCRIPTS / "winlux-browsers.sh").read_text(encoding="utf-8")
nav_settings = (SCRIPTS / "winlux-settings.py").read_text(encoding="utf-8")
nav_installer = (SCRIPTS / "winlux-installer.py").read_text(encoding="utf-8")
nav_first = (SCRIPTS / "winlux-first-boot.sh").read_text(encoding="utf-8")
nav_init = (SCRIPTS / "build-live-iso.sh").read_text(encoding="utf-8")
nav_control = (SCRIPTS / "winlux-control.sh").read_text(encoding="utf-8")
nav_root = (SCRIPTS / "winlux-root.py").read_text(encoding="utf-8")
nav_auto = (SCRIPTS / "winlux_autoinstall.py").read_text(encoding="utf-8")
ok(f"ids='{' '.join(nav_ids)}'" in nav_sh and f"case \"$1\" in {nav_alt}) return 0" in nav_sh,
   "winlux-browsers : huit navigateurs, ordre alphabétique des noms (#1716)")
ok(all(f'"{i}"' in nav_fetch for i in nav_ids if i != "tor") and 'TOR_ID = "tor"' in nav_fetch
   and 'TOR_FINGERPRINT = "EF6E286DDA85EA2A4BA7DE684E2C6E8793298290"' in nav_fetch,
   "winlux-browser-fetch : dépôts officiels et clé de Tor Browser épinglée (#1716)")
ok("tor_fingerprint=EF6E286DDA85EA2A4BA7DE684E2C6E8793298290" in nav_sh and "tor_signature_ok" in nav_sh
   and "VALIDSIG" in nav_sh, "Tor Browser : signature revérifiée avant extraction (#1716)")
ok('BROWSER_NAMES = {"brave": "Brave", "chrome": "Google Chrome", "edge": "Microsoft Edge", "firefox": "Mozilla Firefox",' in nav_settings
   and '"tor": "The Tor Project, Inc."' in nav_settings and 'dialog.set_default_response(Gtk.ResponseType.CANCEL)' in nav_settings,
   "Paramètres > Navigateurs : huit navigateurs, éditeur nommé, Annuler par défaut (#1716)")
ok('("tor", "Tor Browser"), ("vivaldi", "Vivaldi")]' in nav_installer and "grid.attach(button, index % 4, index // 4, 1, 1)" in nav_installer,
   "installeur : huit navigateurs en grille, « Choisir plus tard » présélectionné (#1716)")
ok("operagx Opera_GX tor Tor_Browser vivaldi Vivaldi'" in nav_first and f'case "$BROWSER_CHOICE" in {nav_alt})' in nav_first,
   "premier démarrage : huit navigateurs proposés (#1716)")
ok(f'case "$choice" in {nav_alt}) ;;' in nav_init and f"grep -E '^({nav_alt})$'" in nav_init,
   "init : navigateur choisi et navigateurs en attente reconnus (#1716)")
ok(all(f"browser-{v}-{i}" in nav_control for v in ("install", "default", "launch") for i in nav_ids)
   and f'browser-(install|default)-({nav_alt})' in nav_root, "winlux-control et winlux-root : huit navigateurs (#1716)")
ok('BROWSERS = ("brave", "chrome", "edge", "firefox", "opera", "operagx", "tor", "vivaldi")' in nav_auto,
   "fichier de réponses : huit navigateurs acceptés (#1716)")
ok(not _re.search(r"first-boot-browser[^\n]*(brave|opera|tor|vivaldi)", nav_installer + nav_first),
   "aucun navigateur choisi d'office (#1716)")
