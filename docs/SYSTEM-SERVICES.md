# Sauvegarde, sessions et bureau à distance

Trois services maison, sans Wine ni dépendance tierce de compatibilité.

## Historique des fichiers (`winlux-backup`)

- Appli « Historique des fichiers » du shell (menu Démarrer).
- Réglages par utilisateur dans `/etc/winlux/backup/<compte>.conf` : dossiers,
  destination (disque monté ou partage `smb://`/NFS), fréquence, rétention.
- Instantanés dans `DEST/WinluxFileHistory/<hôte>/<compte>/AAAAMMJJ-HHMMSS/`.
  Les fichiers inchangés sont des liens physiques vers l'instantané précédent
  (`rsync --link-dest`, ou repli BusyBox). Écriture dans `.partial` puis
  renommage atomique.
- L'ISO lance `winlux-backup auto-run` chaque minute ; chaque compte n'est
  sauvegardé que selon sa fréquence. Rien ne tourne pendant une mise à jour.
- La section « Restauration système » renvoie vers les points de restauration
  (`winlux-restore`), qui couvrent le système et non les documents.
- Test : `sh scripts/test-backup.sh`.

## Sessions multi-utilisateur (`winlux-sessions`)

- Menu du compte dans Démarrer : Verrouiller, Se déconnecter, Changer
  d'utilisateur.
- Chaque compte ouvert a son propre Xorg sur son VT ; le changement passe par
  `chvt` sans fermer la session précédente. Registre : `/run/winlux/sessions/`.
- Test : `sh scripts/test-sessions.sh`.

## Bureau à distance (`winlux-remote`)

- Serveur RFB écrit en Rust (`runtime/src/remote/`) : capture X11, clavier et
  souris par XTEST, curseur par XFIXES, encodages Raw/Hextile.
- Sécurité RSA-AES (types 129 et 5 de TigerVNC) : le client vérifie
  l'empreinte de la clé du serveur, puis s'authentifie avec un compte Winlux.
  Tout le trafic est chiffré en AES-EAX. Crypto maison testée sur les vecteurs
  officiels (FIPS 180/197, EAX).
- L'ancien mot de passe VNC (non chiffré) est désactivé par défaut.
- Paramètres → « Bureau à distance » : activer, port, contrôle complet ou
  affichage seul, comptes autorisés. Les comptes sans mot de passe sont refusés.
- Client testé : TigerVNC 1.13 (`vncviewer IP:5900`).
- Test : `sh scripts/test-remote.sh` (après `cargo build --release -p winlux`).
