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

## Bureau sous le compte de l'utilisateur

Sur un système installé, le bureau ne tourne plus en root : un `.exe` lancé
d'un double-clic tourne sous le compte créé à l'installation (`ps -o user`).

- L'init lit `/etc/winlux/install-config.json` (`username`, `autologin`). Si le
  compte existe (UID ≥ 1000), il démarre en root seulement les services
  système (`winlux-system-services` : D-Bus système, polkit, udisks,
  NetworkManager, CUPS, Bluetooth), les deux relais root (`winlux-power serve`
  pour arrêter et redémarrer, `winlux-root serve` pour le reste), puis le
  bureau sur `:0` sous le compte, par `setpriv`, avec `winlux-user-session`
  (son PipeWire, Thunar, picom, barre des tâches, Bienvenue, verrouillage,
  presse-papiers commun, lecteurs réseau, état des Paramètres, écrans).
- Connexion automatique : la session s'ouvre directement. Sinon, elle s'ouvre
  verrouillée et demande le mot de passe du compte.
- `/etc/winlux/users.conf` reprend les vrais comptes (`nom:root|user:shadow`) :
  plus aucun mot de passe en clair, `shadow` renvoie à `/etc/shadow`.
- `/run/winlux` est collant et ouvert à tous (comme `/tmp`), avec
  `fs.protected_symlinks=1` : chacun y écrit ses fichiers d'état sans toucher
  à ceux des autres. Les journaux du bureau sont dans
  `/run/winlux/session-<compte>/`.
- Relais root `winlux-root` (`/run/winlux-root.sock`, appelant vérifié par
  `SO_PEERCRED`, seulement les comptes de `users.conf`) :
  - `conf` : les préférences du bureau dans `/etc/winlux` (thème, clavier,
    confidentialité, verrouillage…), liste fermée de fichiers et de clés ;
  - `control` : les actions système connues de `winlux-control` (réseau,
    Bluetooth, fuseau, écrans, alimentation, mises à jour, sauvegarde,
    sessions…). Les actions d'administrateur (installer un navigateur, carte
    graphique, points de restauration, journal effacé, partages, bureau à
    distance) demandent d'abord le Contrôle de compte d'utilisateur
    (`winlux-uac --elevate`, mot de passe d'un administrateur, élévation de
    5 minutes pour ce compte) ;
  - `exec` : `apt-get` et `snap` pour la Boutique, options APT sûres
    seulement ; installer ou désinstaller demande le Contrôle de compte ;
  - `check`/`elevate` : mot de passe vérifié par `crypt()` sur `/etc/shadow`
    (écran de verrouillage, Contrôle de compte, bureau à distance). Une
    vérification réussie met à jour le mot de passe réseau des partages.
- Disques branchés : éjecter ou réparer depuis le bureau passe par le service
  des volumes (root) via `/run/winlux/disk-action.txt`.
- Session Live : inchangée, elle tourne en root (`/etc/winlux/live-session`).
- Dépannage : `winlux.root-session=1` sur la ligne du noyau rouvre l'ancien
  bureau root d'un système installé.
- Test : `python3 -B scripts/test-root.py`.

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
