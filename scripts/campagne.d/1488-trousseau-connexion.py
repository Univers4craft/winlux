# #1488 : le trousseau de secrets (libsecret) est créé et ouvert à la
# connexion, sans gcr-prompter « Default Keyring » en boucle.
keyring_py = (SCRIPTS / "winlux-keyring.py").read_text(encoding="utf-8")
lock_py = (SCRIPTS / "winlux-lock.py").read_text(encoding="utf-8")
user_session = (SCRIPTS / "winlux-user-session.sh").read_text(encoding="utf-8")
ok("CreateWithMasterPassword" in keyring_py and "UnlockWithMasterPassword" in keyring_py
   and "sys.stdin.read()" in keyring_py
   and "open_keyring(self.entry.get_text())" in lock_py
   and "winlux-keyring unlock" in user_session
   and "winlux-keyring unlock" in (SCRIPTS / "winlux-native-session.sh").read_text(encoding="utf-8")
   and " winlux-keyring " in (SCRIPTS / "stage-desktop-components.sh").read_text(encoding="utf-8")
   and '"gnome-keyring"' in (SCRIPTS / "prepare-desktop-packages.py").read_text(encoding="utf-8")
   and (SCRIPTS / "test-keyring.py").is_file(),
   "Trousseau de secrets ouvert à la connexion, sans demande en boucle (#1488)")
