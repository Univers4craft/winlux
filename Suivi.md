# Suivi du projet Winlux

> **Ce fichier est la source de vérité pour le suivi.** Il doit être mis à jour à CHAQUE action concrète.

## Statistiques

- Réalisé : **259**
- À faire ou en cours : **43**
- Total : **302**
- Progression : **85.8%**
- Dernière mise à jour : 01/11/2026 (audit scripts #214 hors fil de release, audit Winsock/registre #212, audit kernel32/CRT #211, audit runtime graphique #210, audit Paramètres du shell #209, audit complet du code #206, audit : menu Démarrer et barre des tâches du shell PE, audit décodeurs #213, toutes les PRs jusqu'à #135 mergées, PR 2 Paint.NET #143, issues #77 à #98 fermées, nouvelles issues #137 à #140, écran de verrouillage et autorisations des applications #158, presse-papiers Windows ↔ Linux #161, applis Windows au bureau #152, identité visuelle #172, disques branchés #169, Éditeur du Registre #185, installateurs NSIS/Inno et Applications installées #179, WIC, Direct2D et DirectWrite maison pour Paint.NET, exceptions C++ MSVC, outils système #180, #197, #198, #199 (issue #178 close), chemins et droits Windows ↔ Linux, UAC #175, glisser-déposer Windows ↔ Linux #189, partage de dossiers et transfert des réglages #174, portable : veille, écrans multiples et pavé tactile #147, réseau local partagé #168, barre native : horloge, calendrier, modes d'alimentation, mélangeur audio #228, stabilité de la session native #203)

## À FAIRE (par priorité)

### Priorité IMMÉDIATE (★★★★★)

- [x] **Localisation multilingue** : Module i18n (locale.rs) avec 150 chaînes en FR/EN/ES/DE. Page Localisation dans Paramètres (langue, clavier, fuseau horaire).
- [x] **Interface multilingue** : Remplacement des chaînes françaises par `tr()` dans desktop.rs, explorer.rs, wm.rs, settings.rs.
- [x] **Actions contrôle localisation** : winlux-control.sh gère lang-*/keyboard-*/tz-*, 8 dispositions clavier, locale.conf.
- [x] **Installateur multilingue** : Choix de la langue et du clavier lors de l'installation sur disque vierge.
- [x] **Session multilingue** : xsession-winlux lit locale.conf et exporte WINLUX_LANG au démarrage.
- [x] **Gestion fuseau horaire** : timedatectl/ln -sf dans winlux-control.sh, 12 fuseaux courants dans l'interface.
- [ ] **Corbeille fonctionnelle** (#10) : l'envoi vers la corbeille existe ; reste à permettre "Supprimer" -> corbeille, restaurer, vider. Très attendu.
- [ ] **Copie avec progression + pause/annuler** (#11) : UX visuelle (barre de progression + vitesse, ETA) dans l'Explorateur.
- [ ] **Centre de réglages rapides plus complet** (#12) : réseau (#51) et volume (#64) livrés ; reste la luminosité, la batterie (#80) et l'affichage "Ethernet virtuel" sur VM sans Wi-Fi.


### État actuel

- [x] ISO v0.2.12 réparée pour la v0.2.13 (#200) : carte filaire laissée « non gérée » par NetworkManager (réglage Ubuntu Netplan) et DNS de QEMU figé dans l'image, donc pas de réseau ni de mise à jour ; secours DHCP qui ne rallumait pas une carte éteinte ; installateur sans ses modules Python (`/usr/lib/winlux/python` exclu du paquet) ; raccourci « Installer Winlux » dépendant de pkexec absent de l'image ; mise à jour qui se repose sur l'API GitHub quand le manifeste manque.
- [x] Identité visuelle (#170) : logo maison « Prisme » (plus aucun logo Microsoft), écran de démarrage du bureau, thème GRUB assorti, fonds « Winlux Nuit » / « Winlux Aube », page À propos avec matériel réel, son « Prisme ». Voir `docs/IDENTITE-VISUELLE.md`.
- [ ] Identité visuelle, suite (#170) : écran pendant le chargement du noyau, sons système assortis, jeu d'icônes cohérent.
- [x] Lecteurs réseau : FTP, FTPS, SFTP, WebDAV(S), SMB, NFS et S3 montés comme un disque avec lettre (`winlux-drives`, page Paramètres « Lecteurs réseau »), mots de passe chiffrés ChaCha20-Poly1305, reconnexion automatique et état hors ligne publiés dans le registre commun `mounts.tsv`. Seuls les protocoles dont l'outil de montage est présent sont proposés. Reste : modules noyau cifs/nfs dans l'ISO live (SMB passe par rclone en attendant, NFS masqué).
- [x] Disques branchés (#168) : clés USB, disques externes et partition d'un Windows existant montés avec une lettre (`winlux --volumes`), NTFS en lecture/écriture, droits de l'utilisateur, même lettre au même volume, veille prolongée / démarrage rapide / volume « sale » détectés → lecture seule + message clair, bouton Réparer dans Gestion des disques, BitLocker expliqué (`docs/VOLUMES.md`).
- [x] Réseau local partagé Linux ↔ Windows (#168) : WinSock traduit IPv6 (AF_INET6 23↔10), multidiffusion et `IPV6_V6ONLY` ; iphlpapi `GetExtendedTcpTable`/`GetExtendedUdpTable` avec PID ; page « Ports ouverts » du pare-feu avec le nom de l'appli et sa portée ; proxy commun (WinHTTP, registre Internet Settings ↔ `/etc/winlux/proxy.conf` et session Linux) (`docs/RESEAU-LOCAL.md`).
- [x] Écran de verrouillage de la session native au style Windows 11 (`winlux-lock`) : Windows+L, Démarrer > Verrouiller, inactivité, mot de passe au réveil, réglages dans Paramètres > Confidentialité et sécurité (#158).
- [x] Permissions des applications Windows : demande claire (dossiers personnels, webcam, micro, écran ; réseau autorisé d'office mais blocable), choix mémorisé et révocable dans Paramètres > Confidentialité et sécurité (#158).
- [ ] Exécuter le bureau de l'ISO en utilisateur non privilégié avec autorisation ciblée des actions système (le script Xsession existe, mais l'init lance encore le bureau en root).
- [ ] Signer les mises à jour et utiliser des racines A/B pour les coupures d'alimentation (voir `docs/AB_UPDATES.md`).

- [x] Stabilité de la session native (testée dans une VM de l'ISO v0.2.12, captures dans `docs/captures/stabilite-session/`) : menu Démarrer qui ignorait les clics une fois sur deux (saisie du pointeur), Paramètres en une seule fenêtre qui s'ouvre sur la page demandée, un bouton de barre des tâches par fenêtre (plus de menu de regroupement), compositeur picom (plus de traînées en déplaçant une fenêtre, ombres, coins arrondis), traductions fr/en/es/de conservées dans l'image (bureau, Corbeille, éditeur en français).  Corbeille d’un nouveau profil créée à l’ouverture de session (l’Explorateur affichait « Ce dossier est introuvable »).
- [x] Explorateur en session native : les fichiers texte (Corbeille comprise) s'ouvrent dans Mousepad avec leur contenu via l'association Linux, et Suppr dans la Corbeille demande un second appui avant la suppression définitive (fiche .trashinfo effacée aussi).
- [ ] Repeinture de l'Explorateur après un Expose partiel (fenêtre noire jusqu'au survol après le redémarrage du compositeur, `gui_x11.rs`).
- [x] Finition Windows 11 de la session native (1re passe) : menu Démarrer ouvert au-dessus du bouton quand la barre est à gauche, cadre de fenêtre sans coins noirs en bas et titre en graisse normale, volet latéral de la Corbeille (Thunar) lisible et vue détaillée. Captures dans `docs/captures/finition-session/`.
- [ ] Finition Windows 11, suite : avertissement « compte administrateur » de Thunar et Mousepad (bureau lancé en root), types de fichiers en anglais dans Thunar, fenêtre de l'Explorateur placée en partie sous la barre des tâches.

### Priorité 1 — Fiabiliser le runtime

- [ ] Valider la veille, la veille prolongée (`winlux --power hibernate-setup`) et le capot sur un vrai portable installé ; en VM seule la veille ACPI S3 est testable.

- [ ] Continuer à remplacer les implémentations partielles de `kernel32`, `msvcrt` et `ntdll`; ces modules ne constituent pas des DLL Windows complètes.
- [ ] Implémenter une pile de secours réellement sûre pour les débordements de pile; le gestionnaire de signal est sans allocation/verrou, mais la protection contre le débordement n’est pas validée.

### Priorité 2 — Compatibilité Windows

- [ ] Compléter COM/OLE : activation depuis le registre, serveurs hors processus, automatisation et objets OLE applicatifs.
- [ ] Implémenter `StartServiceCtrlDispatcher` et le canal de contrôle/notification des vrais services Windows.
- [ ] Annuler les I/O déjà commencées et tester exhaustivement les buffers/handles invalides.
- [ ] Compléter le cycle de vie des processus : héritage de handles, attributs étendus et suivi/récolte robuste des enfants.
- [ ] Ajouter le partage d’objets nommés et de fichiers entre processus invités.
- [ ] Continuer `ntdll` : exceptions/unwinding, TLS statique des DLL chargées dynamiquement sur chaque thread, sections et primitives bas niveau manquantes.
- [ ] Faire tourner Paint.NET (#138) : runtime embarqué par l'appli (coreclr), puis WIC, Direct2D/DirectWrite et WPF. Étapes et blocages mesurés dans `docs/PAINTNET.md`.
- [ ] Ajouter les DLL système nécessaires aux logiciels ciblés et mesurer leur compatibilité; la présence de stubs ne garantit pas l’API.
- [ ] Brancher Media Foundation/DirectShow des `.exe` sur le moteur multimédia du runtime (`winlux_media.rs`) : le lecteur du shell lit déjà tous les formats, mais `mfplat`/`quartz` restent des bouchons pour les programmes Windows.
- [ ] Implémenter le vrai déchargement/référencement de DLL dans `FreeLibrary` (elles restent mappées jusqu’à la fin du processus).
- [ ] Faire tourner des programmes C++ MinGW (exceptions : `RtlUnwindEx`, `RtlVirtualUnwind`, `RtlLookupFunctionEntry`) et des exécutables MSVC/clang-cl réels.

### Priorité 3 — Double-clic et intégration Linux

- [ ] Valider l’installation réelle sur l’hôte (`sudo integration/install.sh`) ; les tests en racine temporaire ne constituent pas cette preuve.
- [ ] Tester l’enregistrement binfmt sur une machine Linux réelle ; script de diagnostic et configuration présents, résultat réel non établi par cet audit.
- [ ] Tester le double-clic depuis GNOME, KDE et XFCE ; la présence de `test-binfmt-desktops.sh` ne prouve pas ces trois sessions.

### Priorité 5 — Thème Windows 11

- [ ] Navigation clavier complète (Tab entre les contrôles) dans toutes les fenêtres du shell ; la page Accessibilité et le Narrateur sont déjà utilisables au clavier.
- [ ] Revalider le rendu du bureau courant en 1080p, 1440p et 4K, avec captures et interactions ; les tests X11 généraux ne prouvent pas cette matrice complète.
- [ ] Démarrer avec un bureau vide ; rendre les icônes système facultatives et
- [ ] Ajouter la création explicite de raccourcis d'applications, leur

### Priorité 6 — Qualité et livraison

- [ ] Vérifier et embarquer toutes les notices et les obligations de fourniture de sources pour une distribution complète ; NOTICES seul n'est pas une preuve d'empaquetage exhaustif.
- [ ] Valider une exécution complète du workflow de release modifié sur GitHub ; ses étapes de construction et tests ne constituent pas une publication déjà effectuée.
- [ ] Signer les mises à jour (Ed25519) et implémenter le schéma A/B réel (voir `docs/AB_UPDATES.md`).
- [ ] Committer une première version stable après validation du pipeline complet.


## EN COURS

### Pull requests ouvertes

Aucune au 31/10/2026 : toutes les PRs du jour sont mergées sur master.

### Chantiers actifs (issue ouverte)

- [ ] Release v0.2.12 en préparation ; v0.2.7 à v0.2.11 ont échoué (paquets Qt 6 renommés, base APT dans QEMU, bureau bloqué sans Bluetooth, ISO > 2 Gio), corrigés par #128, #132, #142 et la compression xz (#32).
- [ ] Faire tourner Paint.NET 5.1 sur le runtime .NET qu'il embarque : étape 1 livrée (#133), restent coreclr (exceptions MSVC, SEH x64), WIC, Direct2D/DirectWrite et WPF (#138, `docs/PAINTNET.md`).
- [ ] Traduire toutes les applications en FR/EN/ES/DE : lots #120, #130, #131, #134 et #135 livrés, reste les applis encore en une seule langue (#139).
- [ ] Traduire réellement l'installateur : choisir English ne traduit aujourd'hui que la barre des étapes (#140).
- [ ] Partage de dossiers : valider dans une VM avec un vrai PC Windows (panneau Réseau via wsdd) et un Mac (Finder) sur le même réseau (#171).
- [ ] Démarrage sous 10 secondes et mode jeu : temps de démarrage mesuré et affiché livré, mode jeu livré, reste la réduction des étapes les plus longues (mesure en VM KVM) (#153).
- [ ] Suite de l'audit du 01/11/2026 : bugs signalés dans les fichiers gelés par la file de merge (menu Démarrer « Toutes les applications », boutons de la barre des tâches, Corbeille sans confirmation, enregistrement de l'Éditeur, Alt+Tab), threads et synchronisation du runtime, IPv6/WSAPoll/WSAEventSelect, robustesse des décodeurs d'images et du CLR, scripts ISO (pkexec, DNS, binfmt) ; voir les issues ouvertes par l'audit.
- [ ] Compatibilité Linux ↔ Windows du texte, des polices, de la langue, du clavier et de l'heure : livré dans la PR de #183 ; restent les clés de registre dynamiques, `ToUnicode`/`VkKeyScan` selon la disposition et les pages 1250/1251/866 (#183, `docs/INTL.md`).
- [ ] Direct3D, suite de #92 : shaders DXBC/DXIL, profondeur et alias de ressources placées en D3D12, décodage matériel Media Foundation, codecs DXVA2 autres que H.264, validation sur vrai GPU (#137).

## DÉJÀ FAIT

### Livré par pull requests (fin octobre 2026)

- [x] Démarrage : GRUB ne fait plus attendre 10 s « Press any key » en live (recherche de l'installation avant le menu) et le menu attend 2 s au lieu de 5. Mesure KVM : 53,0 s → 40,3 s avec #155, 9,4 s après le noyau (#153, `docs/PERFORMANCES.md`).
- [x] Audit du 01/11/2026, scripts (#214, partie hors fil de release) : mise à jour en place (`dpkg --audit` non bloquant au démarrage, `.deb` installé sous son nom définitif et non `.part`, verrous dpkg jamais créés par `busy()`), initramfs (échecs `modprobe` visibles, applets `loadkmap`/`reboot`/`sync` vérifiées, squashfs reproductible via `-mkfs-time`/`-all-time`), shim `systemctl` (lignes Exec* confiées à `sh -c` sans `eval`, pid 0 jamais « vivant »), hook APT sans démarrage d'unités en chroot ou sous systemd, `winlux-hw-detect` sur `lsblk -P` (modèles et points de montage avec espaces, `DISK_COUNT` exact), points de restauration avec `/etc/shadow`/`gshadow`, `winlux-journal` (double « 0 »), `winlux-browsers` (trap avant le verrou), `winlux-launch-exe` (`shift` sans argument, C:\ = `WINLUX_DRIVE_C` comme le runtime), `winlux-tray` (capacité batterie illisible), `winlux-lock` (marqueur en `O_EXCL`), `winlux-native-config` (comptes système par `useradd`/`groupadd`), premier démarrage (`USER_GROUPS`, thème écrit dans `preferences.conf` lu par la session, fichier `autologin-user` orphelin retiré), installateur (`/etc/winlux/locale.conf` et `input.conf` écrits, lanceur sans `pkexec` : élévation faite par l'installateur lui-même), paquet APT (`usr/lib/winlux/python`, `xorg.conf.d` Winlux, thumbnailer), menu Whisker (`lock-now`, `poweroff`), dossiers XDG réels dans Outils système, `Cargo.toml` alignés sur `VERSION` (0.2.12).
- [x] Audit du 01/11/2026, réseau et système (#212) : Winsock traduit `AF_INET6` (23 ↔ 10) dans socket, bind, connect, sendto, accept, recvfrom, getsockname, getaddrinfo, inet_pton/ntop et WSAStringToAddress (les sockets IPv6 échouaient toutes) ; drapeaux `WSAPoll` traduits ; `WSAWaitForMultipleEvents` surveille les sockets associées par `WSAEventSelect` (FD_WRITE/FD_CONNECT signalés une fois comme sous Windows) ; options `IPPROTO_IP`/`IPV6`/`TCP` traduites ; `GetNameInfoW` sans sous-dépassement. Registre : `create()` revérifie sous verrou, `RegNotifyChangeKeyValue` asynchrone s'arrête à la fermeture de la clé, `DeleteService` supprime l'arbre ; `StrStrIW` sans décalage ni panique sur les caractères qui changent de longueur ; `parse_duration` sans débordement ; gestionnaire de plantage sans allocation ; `mounts.tsv` fsync avant rename ; `/proc/self/mounts` entièrement décodé ; caméra/micro/écran refusés quand on ne peut pas demander ; `::ffff:127.0.0.1` reconnu local ; le pare-feu dit quand les règles par application sont ignorées (cgroup v2 absent).
- [x] Audit du 01/11/2026, kernel32/ntdll/CRT (#211) : `CreateThread` rend l'identifiant que `GetCurrentThreadId` donne dans le thread et `ExitThread` signale le handle avec son code (retour au pont de thread par un setjmp maison, TEB et pile de signal libérés à la fin de chaque thread) ; `ReadFile`/`WriteFile` avec OVERLAPPED ne sont asynchrones que sur un fichier ouvert avec `FILE_FLAG_OVERLAPPED` (sinon synchrones à l'offset) ; `DuplicateHandle` duplique vraiment les objets de synchronisation (nouveau numéro, même objet, `DUPLICATE_CLOSE_SOURCE`) ; `GlobalAlloc(GMEM_FIXED)` rend le pointeur (`GlobalLock` le rend tel quel) ; SList sous le verrou global de ntdll, SRW bloquants (Mutex + Condvar), variables de condition et SRW partagés entre kernel32 et ntdll (`Rtl*` délèguent), `InitOnce` sans course (état PENDING + attente, `InitOnceExecuteOnce`), `HeapReAlloc` sous le verrou du tas, minuteurs `CreateWaitableTimer` attendables ; `FreeLibrary` appelle `DllMain` hors du verrou MODULES, un module dont la liaison échoue est retiré de la liste, chargeur sérialisé par un verrou réentrant ; CRT : stdout/stderr vidés avant toute lecture de stdin, `scanf`/`fscanf` ne consomment que ce que le format demande (EOF = -1), `fseek`/`ftell` en 32 bits (`_fseeki64`/`_ftelli64` en 64), `swprintf` dans sa forme MSVC sans taille (`_swprintf_c`, `swprintf_s`), `strtol`/`strtoul`/`strtoll`/`strtoull` saturés avec `ERANGE`, `_fileno` rend le vrai descripteur, `__cxa_atexit` exécuté à la sortie en ordre inverse ; libgcc : contexte de levée par thread et personality qui filtre par type de `catch` (comparaison des noms de `type_info`) ; libstdc++ : `__cxa_begin_catch`/`__cxa_end_catch` avec compte de gestionnaires et relance, `__cxa_guard_acquire` sans course ; mineurs : `GetModuleFileNameA/W` tronquent et renvoient nSize + `ERROR_INSUFFICIENT_BUFFER`, `GetFileType` (disque/caractère/tube réels), `FormatMessage` avec `FROM_STRING` et arguments nuls, `GetModuleHandleExW` `FROM_ADDRESS` strict, `MapViewOfFile` refuse un décalage au-delà de la taille, `GetComputerNameW` signale un tampon insuffisant, `NtQuerySystemInformation` (classes 0, 1, 3, 8) ; 18 tests unitaires.
- [x] Audit du 01/11/2026, runtime graphique (#210) : `GetMessage` dort par `poll()` sur la socket X11 et un `eventfd` d'éveil par file de messages (plus d'interrogation toutes les 5 ms, verrou du backend relâché pendant l'attente) ; dégradé du thème mis en cache par hauteur de fenêtre et invalidé quand le contraste des réglages change ; FocusIn/FocusOut en mode NotifyGrab/NotifyUngrab ignorés (le focus ne se perd plus pendant un menu) ; `DestroyWindow` retire minuteurs, capture de la souris, ordre Z et messages en file de la fenêtre ; `WM_PAINT` réémis à file vide tant que `BeginPaint`/`ValidateRect` n'a pas validé la zone (et `DefWindowProc` valide) ; minuteurs rattachés à leur thread (`KillTimer`, `WM_TIMER` et l'attente ne concernent que lui) ; lignes clippées au DC par Cohen–Sutherland en i64 avant Bresenham ; `GetWindowText`/`GetClassName`/`GetTextFace` tronquent avec NUL final et renvoient le nombre copié ; `DrawText` avec `count < 0` = chaîne terminée par NUL ; `wsprintfA/W` réels (moteur printf de la CRT, 1024 caractères) ; régions GDI (`CreateRectRgn`, `GetRgnBox`, `PtInRegion`, `OffsetRgn`, `CombineRgn`, `DeleteObject`) comme vrais objets au lieu du handle 1 ; `GetDeviceCaps` HORZRES/VERTRES et `GetMonitorInfo` sur l'écran réel ; ordre Z stable des fenêtres (création/activation, `EnumWindows`, `GetWindow`) ; polices TrueType : `cap_height` sans débordement u16. Presse-papiers : cache par format déjà présent sur master.
- [x] Audit du 01/11/2026, Paramètres (#209, partie settings.rs) : les clics touchent le bouton affiché (registre des boutons dessinés au lieu d'un index de grille : Personnaliser, Assistant, Réglages de l'écran, claviers, fuseaux, Éléments masqués et Extensions déclenchaient le mauvais réglage ou rien) ; pages Apparence, Effets visuels, Localisation, Gestionnaire de fichiers et Son avancé remises en page (les cartes recouvraient des boutons, Thème sombre/clair et Deutsch étaient cachés) ; la molette fait défiler les 8 dispositions de clavier et les 12 fuseaux ; la détection du choix de navigateur ne relance plus system_tick.
- [x] Audit du code du 01/11/2026, corrections nettes : l'Éditeur ne gèle plus à 99 % CPU quand une recherche ne trouve rien ; `GetMessage`/`PeekMessage` livrent `WM_QUIT` (et le `MSG`) ; les exports `user32`/`gdi32` « ext » n'écrasent plus de vraies implémentations par des stubs (`Rectangle`, `ExtTextOut` ETO_OPAQUE, `GetObjectType`, `GetPixel`…) ; `FindResourceA` n'utilise plus un pointeur pendouillant ; `bcrypt` calcule de vrais SHA-1/SHA-256 au lieu de zéros ; Winsock : disposition x64 de `fd_set` (`select` fonctionnel), bornes `FD_SET`, `WSAENOTSOCK`, `MSG_WAITALL`, `servent`, codes d'erreur, `SO_ERROR`, `WSA_FLAG_OVERLAPPED` ne rend plus la socket non bloquante ; FLS et TLS partagent un seul espace d'index ; réveils perdus corrigés sur les variables de condition (kernel32 et ntdll) ; `MoveFile(Ex)` refuse d'écraser sans `MOVEFILE_REPLACE_EXISTING` (erreur 183) ; `CreateFile` OPEN_ALWAYS/CREATE_NEW en lecture seule crée bien le fichier ; Calculatrice sans débordement de `GetTickCount` ; Bloc-notes/Éditeur du Registre ne repeignent plus à chaque mouvement de souris ; outils système : bouton Alimentation vers la bonne page ; `winlux-control` : `volume-set <niveau>` et `shutdown` fonctionnels, `$2` sous `set -u`.
- [x] Audit du 01/11/2026, shell PE (issues #207, #208) : mode « Toutes les applications » du menu Démarrer utilisable (liste alignée sur le clic, bornée, sans grille épinglée dessous, retour aux épinglées à chaque ouverture) ; tous les boutons de la barre des tâches ouvrent ou basculent leur application ; tuiles « Ce PC » et « Programmes .exe » actives, « Programmes .exe » ouvre Applications installées, « Mises à jour » ouvre la bonne page ; menu Alimentation cliquable sur toute sa largeur ; « Fermer » du menu contextuel ferme ; curseur de volume envoie le niveau ; Échap ferme les volets réseau/volume ; Alt+Tab bascule au relâchement d'Alt ; boîte Exécuter : OK cliquable, clic dehors referme, le « r » de Win+R et 260 cases parasites ne s'affichent plus ; liste « Toutes les applications » sans commandes absentes de l'ISO et message « Application introuvable » en cas d'échec ; le Terminal ne fait plus planter le bureau (écran de 1,7 Mo construit sur une pile de 1 Mo).
- [x] Audit du 01/11/2026, décodeurs (#213) : vérifications de bornes avant toute allocation ou lecture dans les décodeurs d'images (PNG/APNG, GIF, JPEG, TIFF, PSD, EXR, XCF, WebP, EPS, glTF), les archives et installateurs (CFB, zip64, CAB, NSIS), les métadonnées CLR (flux, tas, RVA, sections EH, récursion des signatures limitée à 64), WIC, Direct3D 9/11, DXGI, XAudio2 et `SafeArrayCreate` : plafonds de taille, calculs en `checked_*`/64 bits, erreurs propres au lieu de paniques (`panic = "abort"` en release), avec un test d'en-tête malformé par format d'image.
- [x] README réécrit en détail et wiki complet (`wiki/`, 17 pages avec sommaire et menu : but et principes, architecture, bureau, compatibilité, registre, chemins et droits, réseau et disques, Paramètres, installation, publication et mises à jour, construction de l'ISO, tests et CI, contribution, feuille de route, limites, FAQ), publié sur le wiki GitHub par le workflow `wiki.yml` à chaque fusion.
- [x] Applis Windows intégrées au bureau (#152) : associations de fichiers lues dans le registre (HKCU/HKLM\Software\Classes et HKCR, ProgID, OpenWithProgids, OpenWithList, SupportedTypes, UserChoice) → lanceurs cachés `winlux-assoc-*.desktop` (double-clic, « Ouvrir avec » de Thunar et xdg-open, document passé en chemin Windows), type MIME créé pour les extensions inconnues, programme par défaut seulement si l'utilisateur n'a rien choisi ; raccourcis .lnk du Bureau ouvrables (cible, arguments, dossier de travail) ; icônes des .exe/.lnk par thumbnailer tumbler ; fenêtre « Applications par défaut » (Paramètres > Applications) en FR/EN/ES/DE. « Ouvrir avec… » dans le menu contextuel de l'Explorateur (fichiers) : sélecteur GTK `winlux-open-with` qui liste programmes Linux et Windows et peut définir le programme par défaut. Zone de notification : `Shell_NotifyIconA/W` (NIM_ADD/MODIFY/DELETE/SETVERSION, infobulle, état caché, bulles par notify-send, clics transmis au `uCallbackMessage` en format classique ou version 4) via l'assistant GTK `winlux-notify-icon`, et zone « systray » ajoutée au panneau (y compris aux panneaux déjà configurés).
- [x] Gestionnaire des tâches complet façon Windows 11 : processus groupés avec .exe reconnus, tri, recherche, fin de tâche et priorité réelles (`OpenProcess`/`TerminateProcess`/`SetPriorityClass` sur tout processus), graphiques processeur/mémoire/disques/réseau/GPU, historique des applications, démarrage XDG activable, utilisateurs, détails, services (masqués sans systemd), vitesse d'actualisation ; ouvert par Ctrl+Maj+Échap (#178).
- [x] Observateur d'événements façon Windows 11 : journaux Application, Sécurité, Système, Winlux et Plantages lus en vrai (journal du runtime avec nom.exe[pid], journalctl ou dmesg), filtres par niveau et source, recherche, détails, export CSV, « Signaler » ouvre `winlux-report` sur l'événement ; ouvert par `eventvwr` (#197).
- [x] Informations système façon msinfo32 : résumé, matériel (carte mère, processeur, mémoire, disques, GPU, réseau, USB, son), pilotes, environnement logiciel et Winlux, catégories vides masquées, recherche dans tout, export .txt ; ouvert par `msinfo32` (#198).
- [x] Diagnostic de plantage : `winlux-report` joint les étapes avant le problème et des vérifications automatiques (disque, mémoire, versions), et propose au démarrage suivant un rapport « Arrêt du système » après un arrêt anormal (journal noyau, pstore) (#199).
- [x] Mode jeu : un `.exe` Direct3D/OpenGL/Vulkan/XInput déclenche `winlux --gamemode daemon` (gouverneur performances sur secteur, priorité du jeu, sauvegardes et mises à jour en pause et en basse priorité, swappiness et compactage mémoire, veille bloquée), tout est restauré à la fermeture ; réglage dans Paramètres > Processeur, actions affichées selon le matériel (#153, `docs/PERFORMANCES.md`).
- [x] Accents, polices, langue, clavier et heure (#183, `docs/INTL.md`) : pages de code 1252/850/858/437/8859/UTF-8 et API A/W complètes, console en page OEM, noms ZIP Windows (OEM, champ Unicode Path), `winlux --text` (UTF-16, CRLF), locale et formats suivis depuis Paramètres et glibc, disposition clavier XKB → HKL, touches mortes, fuseau TZif et vraies conversions FILETIME (les dates de fichiers des .exe valaient toutes le 1er janvier 2024), polices de `C:\Windows\Fonts` visibles de Linux et remplacements Arial/Segoe UI.
- [x] Temps de démarrage : l'init note chaque étape (`/run/winlux/boot-times`), `winlux --boot-time` en fait le rapport et l'historique, affiché dans Paramètres > Processeur (FR/EN/ES/DE). Edge hors ligne s'installe en arrière-plan et Xorg n'impose plus 3 s d'attente fixe (#153).
- [x] Chemins Windows ↔ Linux (#175, partie 1) : `C:\Users\<nom>` et ses dossiers connus suivent les dossiers XDG (Documents, Bureau…), `SHGetFolderPath`/`SHGetKnownFolderPath`/`GetUserProfileDirectory`, variables `USERPROFILE`/`APPDATA`…, `ExpandEnvironmentStrings`, noms Linux interdits sous Windows codés comme Cygwin, casse stable, chemins longs, `NUL`, UNC, liens symboliques (REPARSE_POINT, `CreateSymbolicLink`), `GetFullPathName` sur le texte, lecture seule réelle ; accents perdus par `CreateFileW`/`LoadLibraryW` & co corrigés. Test PE `winpaths`, `docs/CHEMINS-ET-DROITS.md`.
- [x] Droits Windows tirés des permissions Linux (jetons, SID, `GetFileSecurity`/`GetNamedSecurityInfo`) et UAC maison : manifeste `requireAdministrator` ou `runas` → demande du mot de passe local, fichiers créés laissés à l'utilisateur (#175, partie 2).

- [x] Captures d'écran automatiques et mode machine virtuelle (#156) : `scripts/capture-ui.py` photographie sous Xvfb les 25 pages des Paramètres, le menu Démarrer, les popups, les outils système, Bienvenue et les applis maison PE (bureau, Explorateur, Store, Polices, Planificateur, Pare-feu) ; `scripts/compare-captures.py` les compare à la dernière version verte de master (job CI « Captures d'écran », artefacts `captures-ui` et `captures-ui-diff`, alerte au-delà de 2 % de pixels changés). `winlux-hardware` détecte l'hyperviseur (`/run/winlux/hw/virtual-machine`) et ne compte plus les cartes émulées comme GPU : curseur logiciel conservé, économiseur en GL logiciel, « Suivre la taille de la VM » seulement en VM.
- [x] Partage de dossiers vers Windows, Mac et Linux (#171) : `winlux-share` (Samba, outil standard, caché s'il manque), comptes Winlux = comptes SMB, droits Aucun accès / Lecture / Modification par compte et pour les invités, dossiers système refusés, SMB1 coupé, pare-feu ouvert sur les réseaux privés seulement, annonce wsdd/Avahi. Explorateur : clic droit > « Partager ce dossier… ». Paramètres > Partage et transfert (page 28). Tests `scripts/test-share.sh`.
- [x] Transfert des réglages entre deux PC Winlux par fichier `.wlxprofile`, sans compte en ligne (#171) : `winlux-transfer` (thème, barre des tâches, fond, accessibilité, lecteurs réseau sans mot de passe, applis par défaut, langue, clavier, souris, confidentialité), import en liste blanche, sauvegarde et annulation. Tests `scripts/test-transfer.sh`.
- [x] Direct2D 1.0 logiciel maison (`d2d1.rs`, `d2d_raster.rs`) : géométries, chemins, pinceaux unis, bitmaps, cible de rendu sur bitmap WIC, rendu anticrénelé, transformations et découpe ; `ID2D1DeviceContext` et dégradés à venir (`docs/PAINTNET.md`).
- [x] Exceptions C++ MSVC maison (`cxx_eh.rs`) : `_CxxThrowException`, `__CxxFrameHandler3`, `__C_specific_handler` (`__try/__except/__finally`), test `testc/msvc/eh_msvc.cpp` en CI ; `__CxxFrameHandler4` à venir.
- [x] DirectWrite maison (`dwrite.rs`, `sfnt.rs`) : contours TrueType lus sans dépendance, polices du système et de remplacement des polices Windows, mise en page du texte (coupure, alignements, tests de position), texte dessiné par Direct2D ; mise en forme OpenType et polices CFF à venir.
- [x] WIC maison (`windowscodecs.rs`) : fabrique, décodeurs sur les décodeurs d'images de Winlux, bitmaps verrouillables, conversion de formats, mise à l'échelle, découpe, rotation, `IWICStream` ; encodeurs à venir (`docs/PAINTNET.md`).
- [x] coreclr embarqué démarré : répartition SEH x64 (`seh.rs` : RtlVirtualUnwind, RtlUnwindEx, RtlRestoreContext, VEH, tables de fonctions dynamiques), exceptions C++ MSVC de coreclr, tas du GC, thread du finaliseur, System.Private.CoreLib puis `paintdotnet.dll` chargées et code managé exécuté (images relogées sur 64 Kio, VirtualQuery des piles, VerifyVersionInfo, InitializeContext2, locales nommées) (`docs/PAINTNET.md`).
- [x] Issues fermées le 31/10/2026 car livrées sur master : impression (#77 par #111), audio des .exe (#78 par #115), manettes, webcam, micro et Bluetooth (#79 par #114), alimentation et batterie (#80 par #107), Store et polices (#81, #82 par #123 et #129), planificateur et pare-feu (#83, #84 par #112), presse-papiers Win+V et capture d'écran (#85, #86 par #109), sauvegarde, multi-utilisateur et bureau à distance (#87, #88, #89 par #119), CPU (#90 par #103), GPU (#91 par #104), Direct3D 9/12 (#92 par #127), installateur (#93 par #75, #116 et #118 ; #94 par #76 ; #95 par #100 ; #96 par #101 ; #97 par #105 ; #98 par #102).
- [x] Aussi mergés : signalement de bugs vers GitHub (#121), visionneuse d'images (#126), noms de paquets Ubuntu 26.04 (#128), base APT vérifiée dans QEMU (#132), CLR maison (#73), SEH/TLS/services (#63), localisation (#72), détection du matériel (#61).
- [x] Lanceur .NET autonome : un apphost qui embarque `coreclr.dll` (Paint.NET 5.1.12) traverse hostfxr et hostpolicy jusqu'au chargement de coreclr ; CRT universelle `__stdio_common_*`, chaînes `*_s`, ordinaux oleaut32, `ungetc` (`docs/PAINTNET.md`).

- [x] Gestionnaire de tâches style Windows 11 (#39).
- [x] Enregistreur vocal natif avec capture ALSA (#40).
- [x] Suppression des alertes Bluetooth au démarrage (#41).
- [x] Arrêt et redémarrage propres avec écran de fermeture (#42).
- [x] Centrage dynamique des icônes de la barre des tâches (#43).
- [x] Écran de connexion, invite UAC, verrouillage Win+L et menu alimentation (#44).
- [x] Snap layouts et bureaux virtuels (#45).
- [x] Centre de notifications Windows 11 (#46).
- [x] Thème automatique, transitions et fondu des fenêtres (#47).
- [x] Nettoyage de disque intégré au shell (#48).
- [x] Recherche globale (menu Démarrer + Win+S) (#49).
- [x] Éditeur de texte avancé avec coloration syntaxique (#50).
- [x] Panneau réseau Wi-Fi/Ethernet dans la barre des tâches (#51).
- [x] Support Electron (Claude.exe, ChatGPT.exe) et paint.net (#52).
- [x] Widget météo et gestionnaire de disques (#53).
- [x] Terminal natif intégré au shell (#54).
- [x] Audit UX : layout DPI, performance GDI et détails manquants (#55).
- [x] Cohérence visuelle et thèmes dans toutes les apps (#56).
- [x] Défilement fluide partout (#57).
- [x] Rendu graphique : animations, RoundRect natif, alpha (#58).
- [x] Corrections UX : curseurs, clavier des apps, recherche Start, souris Notepad (#59).
- [x] Glisser-déposer de fichiers et Ctrl+C/V entre Explorer et bureau (#60).
- [x] Paramètres : pages Localisation et Gestionnaire de fichiers (#62).
- [x] Paramètres natifs (GTK) : les 10 pages restées vides en v0.2.12 (Bluetooth, Personnalisation du bureau, Lecteurs réseau, Mises à jour système, Son avancé, Gestionnaire de fichiers, Localisation, Accessibilité, Effets visuels, Programmes Windows) sont branchées sur les vrais services ; page sans service masquée.
- [x] Mixeur de volume dans la barre des tâches et réglages de localisation (#64).
- [x] Registre Windows complet (énumération, clés pré-remplies), COM/OLE étendu, SafeArray, crash handler enrichi (#65).
- [x] WinSock complet pour le réseau des .exe (#66).
- [x] Installateur graphique pour ordinateur vierge, multilingue, GPT BIOS/UEFI, détection matériel (#67).
- [x] Correction du ghosting des fenêtres lors du déplacement (#68).
- [x] Écran bleu (BSOD), journalisation centralisée, observateur d'événements et points de restauration (#69).
- [x] Traduction Direct3D 11 vers OpenGL (#70).
- [x] Direct3D 9 (périphérique, surfaces, textures, VB/IB, shaders SM1.x à 3.0 traduits en GLSL, états fixes), Direct3D 12 (files, listes, tas de descripteurs, PSO, signatures racine, barrières, swapchain DXGI) avec backend Vulkan choisi selon le matériel puis repli OpenGL et processeur, DXVA2 H.264 par VA-API (NVDEC via nvidia-vaapi-driver), section « Jeux et applications Windows (Direct3D) » de la page Paramètres « Graphismes », masquée quand le matériel ne permet aucun choix (#92). Suite PE `d3d_suite`.
- [x] DLL libgcc_s_seh-1 / libstdc++-6 et NtCreateFile/NtOpenFile réels (#71).
- [x] Premier démarrage (OOBE) : thème, choix des apps, compte local avec avatar (#74).
- [x] Compatibilité systemd (shim `systemctl`, hook APT) et passage à Ubuntu 26.04 LTS (#38).
- [x] Suppression des doublons et correction d'un data race (#36).


### État actuel

- [x] Gestion GPU dans Paramètres > Graphismes : détection des capacités réelles (sysfs amdgpu/i915/xe, NVML), mesures en direct, profil/limite de puissance/overclocking seulement si le pilote les accepte, GPU par application sur machine hybride (`gpu=` du profil .exe, `winlux-prime-run`), onglets GPU du Gestionnaire des tâches et overlay de performances en jeu (raccourci configurable). Tout ce que la machine n'expose pas est masqué.
- [x] Gestion de l'alimentation en Rust (`runtime/src/power/`) : batterie lue dans `/sys/class/power_supply` (pourcentage, charge/décharge, autonomie, santé), modes Performances maximales / Équilibré / Économie d'énergie (gouverneur cpufreq, EPP, turbo, luminosité, autosuspend USB, ASPM, SATA), veille et veille prolongée par `/sys/power` avec reprise propre, démon lancé par l'init maison (inactivité écran/veille, capot, bouton, batterie critique), icône batterie avec charge et infobulle, alertes 20 %/10 %, page Paramètres « Alimentation et batterie », API `GetSystemPowerStatus`, `SetThreadExecutionState`, `PowerCreateRequest/SetRequest/ClearRequest`, `PowerGetActiveScheme/SetActiveScheme`, `PowerReadFriendlyName`, `SetSuspendState`, `GetPwrCapabilities` (test PE `power_api`).
- [x] Portable, veille et luminosité (#147) : veille sans perte d'évènement de réveil (`/sys/power/wakeup_count`, nouvel essai si le noyau est occupé), scripts avant/après veille dans `/usr/lib/winlux/sleep.d` et `/etc/winlux/sleep.d`, touches luminosité du clavier (sans double pas quand l'ACPI agit déjà, pas de 5 % sous 20 %), capot fermé avec un écran externe branché : la machine reste éveillée.
- [x] Écrans multiples (#147) : `winlux --display` (politique en Rust, `xrandr` pour la plomberie) : branchement à chaud suivi par `/sys/class/drm` sans sonder les écrans en boucle, disposition étendre/dupliquer/un seul écran, écran principal, position, définition et mise à l'échelle par écran (le bureau dessine à la plus grande échelle, les autres écrans réduits par RandR), réglages mémorisés par ensemble d'écrans, capot fermé avec écran externe : dalle interne éteinte, échelle recommandée selon la densité réelle (EDID). Nouvelle page Paramètres « Écran » avec schéma cliquable. API Win32 `EnumDisplayMonitors`, `GetMonitorInfo(Ex)`, `MonitorFromPoint/Rect/Window`, `SM_CMONITORS`, écran virtuel ; le bureau suit l'écran principal. Reste : valider sur un vrai portable avec écran externe.
- [x] Pavé tactile (#147) : `winlux-touchpad` lit les réglages que le pavé accepte réellement (libinput) : toucher-clic, défilement à deux doigts, clic droit à deux doigts, défilement naturel, désactivation pendant la frappe, vitesse ; réappliqués au début de session et à la reprise de veille. Pincer pour zoomer : gestes XInput 2.4 traduits en Ctrl + molette, comme un pavé de précision Windows. Section Paramètres cachée sans pavé tactile (ordinateur fixe, VM).

- [x] Préserver les noms longs et refuser les chemins tronqués dans Explorer.
- [x] Bloquer les copies dans un descendant, les collisions et les alias de chemins ; tester le couper/coller et les raccourcis.
- [x] Corriger Ctrl+L, les touches Windows, AltGr et la disposition XKB réelle.
- [x] Détecter les images/disques périmés par empreinte des sources et conserver le disque précédent lors d'un rafraîchissement.
- [x] Charger le noyau et la racine persistante du disque depuis GRUB.
- [x] Ajouter DHCP, route, DNS et renouvellement réseau, avec tests isolés.
- [x] Paramètres Linux/Windows, télémétrie réelle et commandes système avec journaux.
- [x] Centre graphique de mise à jour, paquets système complets, jeton pour dépôt privé, automatique, restauration et demande de redémarrage.
- [x] Mise à jour en place protégée (#150) : point de restauration automatique avant l'installation, vérification de chaque fichier installé, version « à l'essai » validée quand le bureau tient une minute, retour automatique à la version précédente si le bureau ne s'ouvre pas (ou au 3e démarrage bloqué), version écartée jamais réinstallée automatiquement. Reste : compteur GRUB si `/init` est cassé, signature Ed25519.
- [x] Ajouter un installateur autonome BIOS/UEFI qui ne nécessite plus l'ISO comme chargeur initial.
- [x] Passer Explorer entièrement aux chemins Unicode Win32 W.
- [x] Charger un exécutable PE32+ x86-64.
- [x] Appliquer les relocations et résoudre les imports de base.
- [x] Initialiser un PEB/TEB minimal via `GS`.
- [x] Fournir les API nécessaires aux tests P1.
- [x] Implémenter les ponts `printf`/`sprintf` variadiques Windows x64.
- [x] Exécuter `hello.exe`, `exitcode.exe`, `p1_suite.exe`, `dll_import.exe`, `delay_import.exe`, `findfiles.exe`, `crt_suite.exe`, `gui_suite.exe` et `gui_demo.exe` sous Winlux.
- [x] Ajouter des tests unitaires couvrant le runtime, le registre, le SCM, l'audio, Winsock, les processus, les fichiers, la CRT, IOCP, COM/OLE et le noyau fenêtres/messages.
- [x] Exécuter les dix PE simultanément avec isolation, temporisation et journaux par processus.
- [x] Ajouter les fichiers binfmt, MIME et `.desktop`.
- [x] Ajouter un premier script de génération ISO.
- [x] Ajouter une base de thème visuel inspirée de Windows 11.
- [x] Historique des fichiers maison : dossiers, destination (disque externe ou partage SMB/NFS), fréquence, rétention, instantanés incrémentaux par liens physiques, restauration des versions et lien vers les points de restauration système (`winlux-backup`, `scripts/test-backup.sh`).
- [x] Multi-utilisateur : verrouillage, changement rapide d'utilisateur sans fermer la session, sessions simultanées sur un Xorg/VT par compte (`winlux-sessions`, `scripts/test-sessions.sh`).
- [x] Bureau à distance maison : serveur RFB `winlux-remote` (capture X11, clavier/souris XTEST, curseur), authentification par compte Winlux chiffrée RSA-AES (TigerVNC/RealVNC), page « Bureau à distance » des Paramètres (`scripts/test-remote.sh`).

### Priorité 1 — Fiabiliser le runtime

- [x] Ajouter des tests unitaires pour le parsing PE et le quoting de la ligne de commande.
- [x] Ajouter un test d’intégration automatisé des trois exécutables de référence.
- [x] Corriger la gestion de `GetTickCount` et utiliser une horloge monotone globale.
- [x] Améliorer la gestion de `VirtualAlloc`/`VirtualFree`, dont l’alignement 64 Ko.
- [x] Isoler les valeurs TLS Win32 par thread.
- [x] Valider les indices TLS, les libérer avec `TlsFree` et invalider les valeurs obsolètes.
- [x] Implémenter `VirtualProtect`, `MEM_DECOMMIT` et le recommit de mémoire.
- [x] Ajouter un test dédié aux exports par nom et par ordinal.
- [x] Déclencher les callbacks TLS et `DllMain` lors de la création de threads Win32.
- [x] Ajouter la validation des références d’imports différés et leur conversion RVA/VA.
- [x] Ajouter un test d’intégration avec une DLL réelle.
- [x] Ajouter le chargement dynamique via `LoadLibraryA/W`, `GetProcAddress` par nom/ordinal et les callbacks d’initialisation.
- [x] Ajouter un test synthétique exerçant effectivement la résolution d’un import différé.
- [x] Ajouter un binaire Windows compilé avec un vrai répertoire d’imports différés.
- [x] Éviter les allocations et verrous dans le gestionnaire de signaux de crash.
- [x] Définir la politique pour les imports manquants : journalisation, arrêt par défaut et mode explicite `WINLUX_MISSING=ignore`.
- [x] Renforcer les vérifications de bornes des sections, imports et IAT avant accès mémoire.

### Priorité 2 — Compatibilité Windows

- [x] Ajouter des DLL intégrées courantes : `user32`, `advapi32`, `ole32`, `bcrypt`, `version` et `shell32`.
- [x] Ajouter la fondation COM/OLE native : appartements par thread, GUID, allocateur COM, mémoire globale et `IStream` partageable sur `HGLOBAL`, avec tests unitaires et PE.
- [x] Enregistrer/révoquer des fabriques COM multi-usages et créer leurs objets dans le processus, avec cookies stables et appels réentrants testés.
- [x] Planificateur de tâches 2.0 pour les `.exe` : `CoCreateInstance(CLSID_TaskScheduler)` → `ITaskService`, dossiers, `RegisterTaskDefinition`/`RegisterTask` (XML), déclencheurs temps/quotidien/hebdomadaire/démarrage/session/enregistrement, `IExecAction`, réglages et tâches en cours (voir `docs/PLANIFICATEUR_PAREFEU.md`).
- [x] Ajouter un diagnostic explicite pour les exécutables gérés dépendant de `mscoree.dll`.
- [x] Affinité, priorités, topologie et NUMA pour les `.exe` : `Set/GetProcessAffinityMask`, `SetThreadAffinityMask` (y compris threads suspendus), `Get/SetThreadGroupAffinity`, `Set/GetPriorityClass`, `Set/GetThreadPriority` (sans priorité irréversible pour un utilisateur normal), `GetLogicalProcessorInformation(Ex)` avec classe d'efficacité P/E-cores, `GetNuma*` ; tests unitaires (`kernel32_cpu.rs`).
- [x] Détecter les en-têtes CLR PE32/PE32+ et les apphosts modernes, puis relayer les assemblies vers `dotnet` ou Mono lorsqu'un runtime Linux compatible est installé.
- [x] Relier les API de base fichiers/répertoires Windows au système de fichiers Linux.
- [x] Ajouter des variantes Unicode `W` pour les chemins, fichiers et variables d’environnement.
- [x] Ajouter `GetFullPathName`, `CopyFile`, `MoveFile`, `CreateDirectory` et `RemoveDirectory` en A/W ou A selon l’API.
- [x] Ajouter `GetTempFileNameA/W`, `GetFileAttributesExA/W` et `SetEndOfFile` avec tests fonctionnels.
- [x] Ajouter l’énumération de répertoires `FindFirstFileA/W`, `FindNextFileA/W` et `FindClose` avec motifs `*`/`?` et tests PE.
- [x] Implémenter les conversions UTF-8 et Windows-1252 avec vérification des buffers.
- [x] Ajouter les API de base `GetSystemInfo`, version OS, horloge FILETIME et plusieurs primitives `Rtl*`.
- [x] Ajouter les premières API GUI (`user32`, `gdi32`) et réseau (`ws2_32`).
- [x] Construire le noyau fenêtres/messages (`gui.rs`) : classes, table HWND, file par thread, appel du `WndProc` invité et palette claire inspirée de Windows 11, avec un test PE `gui_suite.exe` (séquence WM_CREATE/WM_SIZE/WM_PAINT/WM_DESTROY).
- [x] Relier le noyau à un backend X11 réel (`gui_x11.rs`, x11rb) : vraies fenêtres, surfaces claires et accent bleu rendus côté serveur, événements X traduits en messages Win32 (`Expose`→`WM_PAINT`, `ConfigureNotify`→`WM_SIZE`, souris, clavier X11→`VK_*`/`WM_KEYDOWN`/`WM_KEYUP`, `WM_DELETE_WINDOW`→`WM_CLOSE`). `TranslateMessage` produit `WM_CHAR` (ASCII/Latin-1 et BMP) ; tracé GDI et saisie vérifiés par `gui_demo.exe` sous Xvfb. Backend *headless* conservé pour les tests (`WINLUX_GUI=headless`).
- [x] Implémenter `SetWindowPos` et `MoveWindow` : subset de flags `SWP_NOMOVE`/`SWP_NOSIZE`/`SWP_NOREDRAW`, rectangles HWND actualisés, messages `WM_MOVE`/`WM_SIZE`, configuration de la vraie fenêtre X11 et synchronisation des positions/tailles via `ConfigureNotify`; testé par `gui_suite.exe` en headless et sous Xvfb.
- [x] Ajouter le focus clavier par thread (`GetFocus`/`SetFocus`), les notifications synchrones `WM_SETFOCUS`/`WM_KILLFOCUS`, la perte de focus à la destruction et la traduction des événements X11 `FocusIn`/`FocusOut`; testé par `gui_suite.exe` et par injection de focus sous Xvfb.
- [x] Ajouter `GetKeyState`, `GetAsyncKeyState` et `GetKeyboardState` : état thread mis à jour lors du retrait de `WM_KEYDOWN`/`WM_KEYUP`, état asynchrone alimenté par les événements X11, bits pressé/bascule et tableau de 256 touches; tests unitaires, PE et saisie Xvfb.
- [x] Ajouter les variantes Unicode de la boucle de messages et des titres (`GetMessageW`, `PeekMessageW`, `DispatchMessageW`, `PostMessageW`, `SendMessageW`, `Set/GetWindowTextW`) avec tests PE.
- [x] Ajouter la couche de dessin GDI : commandes (`FillRect`, `FrameRect`, `Ellipse`, `Text`, `BitBlt`, `StretchBlt`) accumulées par cycle de peinture puis rejouées par le backend (y compris à chaque `Expose`), pinceaux/plumes/polices (`CreateSolidBrush`, `CreatePen`, `CreateFontA/W`, `SelectObject`, `DeleteObject`, `GetStockObject`, `GetObjectA/W`), `GetSysColor`/`GetSysColorBrush`, `PatBlt` (PATCOPY/BLACKNESS/WHITENESS/PATINVERT/PATAND/PATOR/DSTINVERT), `CreateCompatibleBitmap`/`CreateDIBSection` 32bpp et DC mémoire pour `SRCCOPY`, `SRCPAINT`, `SRCAND`, `SRCINVERT`, `NOTSRCCOPY`, couleurs et mode de fond (`SetTextColor`, `SetBkColor`, `SetBkMode`), `TextOutA`, `Rectangle`, `Ellipse` et `GetDC`/`ReleaseDC`. `gui_demo.exe` vérifie le tracé; rendu et blits contrôlés par capture X11 (`xwd`).
- [x] Relier sockets TCP/UDP, erreurs WSA et résolveur `getaddrinfo` A/W aux interfaces POSIX (tests loopback).
- [x] Ajouter un registre persistant partagé Linux/Windows.
- [x] Registre Windows commun à toutes les applis (chaînes Unicode, HKCR fusionné), `reg.exe`/`regedit.exe` intégrés, import et export de fichiers .reg (#166), et appli « Éditeur du Registre » façon Windows 11 : arborescence, édition des valeurs de tous types, création, renommage, suppression, recherche (F3), import et export, double-clic sur un .reg avec confirmation (voir `docs/REGISTRY.md`).
- [x] Fonctions registre de shlwapi pour les .exe : SHGetValue/SHSetValue, SHDeleteKey, SHRegGetValue, réglages utilisateur puis machine (SHRegGetUSValue, clés HUSKEY), flux SHOpenRegStream2 et associations de fichiers AssocQueryString/AssocQueryKey (voir `docs/REGISTRY.md`).
- [x] Ajouter une couche SCM locale, non privilégiée, qui persiste les définitions et lance/arrête les services en processus utilisateur.
- [x] Ajouter les premiers points d’entrée multimédia `mfplat` et `winmm`.
- [x] Ajouter le décodage WAVE PCM/float et un pont ALSA optionnel pour `PlaySoundA/W` (parseur testé; sortie physique encore à valider sur machine audio).
- [x] Audio des .exe : moteur de mixage maison (48 kHz, 10 ms) avec sortie PipeWire native, PulseAudio, ALSA ou nulle choisie à l'exécution ; waveOut, DirectSound (+ 3D simple et capture), WASAPI (rendu, capture, loopback, partagé/exclusif) et XAudio2 2.7–2.9 ; un flux par .exe dans le mixeur système et curseurs par application dans la fenêtre Volume (voir `docs/AUDIO.md`).
- [ ] Audio : décodeurs ADPCM/xWMA/XMA pour XAudio2, effets XAPO tiers et xapofx, Doppler et cônes DirectSound 3D, X3DAudio.
- [ ] Ajouter PipeWire (pipewire-pulse, wireplumber) à l'ISO pour le mixeur par application ; l'image ne contient aujourd'hui qu'ALSA.
- [x] Ajouter les primitives de synchronisation `Event`, `Mutex` et `WaitForSingleObject`.
- [x] Ajouter `VirtualQuery`, `WaitForMultipleObjects` wait-any/wait-all et les sections critiques récursives.
- [x] Ajouter `ReadFile`/`WriteFile` overlapped avec offsets, `ERROR_IO_PENDING`, signal d’événement et `GetOverlappedResult`.
- [x] Ajouter une annulation best-effort des opérations overlapped en attente avec `CancelIo` et `CancelIoEx`.
- [x] Ajouter un sous-ensemble IOCP : association d’un fichier, `GetQueuedCompletionStatus[Ex]` et `PostQueuedCompletionStatus`.
- [x] Respecter le plafond de concurrence IOCP pour les threads qui retirent les paquets avec `GetQueuedCompletionStatus[Ex]`.
- [x] Libérer le slot actif d’un thread Win32 à sa terminaison, avec test dédié.
- [x] Ajouter `CreateProcessA/W`, les handles de processus, attente et codes de sortie pour des processus Linux/PE enfants (sous-ensemble).
- [x] Lire les blocs d’environnement ANSI ou Unicode de `CreateProcessA/W` et les appliquer au processus enfant.
- [x] Implémenter `CREATE_SUSPENDED`/`ResumeThread` pour les enfants via un lanceur Linux suspendu avant l’exécution du programme.
- [x] Étendre la CRT : chaînes étroites/larges, conversions (`strtol`, `strtod`, `_itoa`, `_itow`), classification de caractères, fonctions mathématiques, `qsort`/`bsearch` et formatage borné (`_snprintf`, `_snwprintf`), avec tests unitaires et PE (`crt_suite`).
- [x] Ajouter les flux `FILE*` de la CRT (`fopen`/`fclose`, `fread`/`fwrite`, `fgets`/`fputs`, `fseek`/`ftell`, `feof`, `fprintf`, `vfprintf`, `vsprintf`, `remove`/`rename`) avec tests unitaires et PE.
- [x] Ajouter les entrées formatées `scanf`/`sscanf`/`fscanf` et leurs variantes `v*` (entiers, flottants, chaînes, jeux de caractères, largeur, `*`, `%n`) avec tests unitaires et PE.
- [x] Sauvegarder/restaurer la position des flux CRT avec `fgetpos`/`fsetpos`, avec tests.
- [x] Continuer la CRT : locale, `_snscanf` et formats positionnels. (_snscanf ajouté, formats positionnels en attente)
- [x] Ajouter `SetDIBitsToDevice`/`StretchDIBits` BI_RGB 1/4/8/16/24/32 bits avec tests de palette et de bornes.
- [x] Poursuivre le rendu X11 : décorations/titre maison si absence de WM; compléter GDI (autres formats DIB/bitmap, ROP restants, polices/métriques complètes et texte sur DC mémoire). (ROP SRCERASE/NOTSRCERASE/MERGEPAINT/PATCOPY/PATPAINT/DSTCOPY ajoutés, métriques de police améliorées)
- [x] Décoder MP3 et AAC ADTS mono/stéréo dans PlaySound, tester limites mémoire, concurrence et sortie ALSA null ; inclure les bibliothèques dans l'ISO.
- [x] Exécuter de vrais programmes C compilés par MinGW-w64 : démarrage de la CRT msvcrt (`__getmainargs`, `_initterm`, constructeurs globaux, `atexit`/`_onexit`, `_iob`/`__iob_func`), `_errno`, verrous `_lock`, environnement, temps (`time`, `localtime`, `strftime`, `clock`), `rand` Microsoft, E/S bas niveau `_open`/`_read`/`_write`, signaux, `_beginthreadex`; compléments kernel32 (console, pages de code, `GetEnvironmentStrings`, `FormatMessage`, `lstr*`, `RaiseException`…). Vérifié par `testc/mingw_suite.c` (40 contrôles) en CI.
- [x] Documenter explicitement les limites : 64 bits uniquement, absence de noyau Windows et compatibilité API partielle.
- [x] Impression : `winspool.drv` maison (OpenPrinter, StartDocPrinter/WritePrinter/EndDocPrinter, EnumPrinters 1/2/4/5/6/8/9, GetPrinter, SetPrinter, Get/SetDefaultPrinter, AddPrinter, DeletePrinter, EnumJobs, GetJob, SetJob, DeviceCapabilities, DocumentProperties) avec imprimante virtuelle « Imprimer en PDF (Winlux) ».
- [x] Impression : pont CUPS par protocole IPP écrit de zéro (socket Unix ou TCP, PeerCred) : découverte des files, imprimante par défaut, envoi des travaux, file d'attente, annulation, ajout IPP Everywhere ou pilote CUPS, détection USB/réseau. Validé contre un vrai `cupsd` (`WINLUX_TEST_CUPS=1 cargo test -p winlux cups_reel -- --ignored`).
- [x] Impression : DC imprimante GDI (`CreateDC`/`CreateIC`, `StartDoc`, `StartPage`, `EndPage`, `EndDoc`, `AbortDoc`, `SetAbortProc`, `ResetDC`, `Escape`, `GetDeviceCaps`) rendu en PDF (texte en polices standard, formes, images RunLength). Test PE `print_suite`.
- [x] Impression : boîtes `PrintDlg`, `PrintDlgEx`, `PageSetupDlg` natives au thème Windows 11 (aperçu, imprimante, copies, orientation, pages, papier, couleur, qualité, recto verso) ; `WINLUX_PRINTDLG=accept` sans affichage.
- [x] Impression : page Paramètres › Imprimantes et scanners (liste, imprimante par défaut, page de test, suppression, file d'attente avec annulation, recherche USB/réseau, ajout par adresse) ; CUPS ajouté aux paquets du bureau de l'ISO.

- [x] Lecteur multimédia natif (`mediaplayer.exe`, `audioplayer.exe` en alias) : vidéo et audio, barre de progression avec aperçu au survol, volume, vitesse 0,25× à 3×, plein écran, sous-titres SRT/ASS/VTT externes et pistes intégrées (texte et PGS/VobSub), pistes audio, chapitres, liste de lecture du dossier, reprise à la dernière position, thème clair/sombre, FR/EN/DE/ES. L'Explorer ouvre l'audio/vidéo dedans ; MIME par défaut dans l'ISO.
- [x] Visionneuse d'images native (`imageviewer.exe`) et décodeurs maison en Rust (`runtime/src/imaging`, aucune bibliothèque externe) : PNG/APNG, JPEG (progressif, CMYK), GIF animé, BMP, TIFF multipage, ICO/CUR/ICNS, WebP (avec et sans perte, animé), QOI, PSD/PSB et XCF (calques), SVG/SVGZ vectoriel net à tout zoom, AI/EPS (aperçu intégré), TGA, PCX, PNM/PAM/PFM, DDS (BC1-BC5), EXR, HDR, SGI, Sun, XBM/XPM, WBMP et RAW (CR2, CR3, NEF, ARW, DNG, ORF, RW2, RAF, PEF, SRW… via l'aperçu intégré). Zoom molette/ajusté/100 %, panoramique, rotation, miroir, pages/calques/images, dossier précédent/suivant, diaporama, panneau EXIF/IPTC/XMP, comparaison avant/après RAW (exposition, niveaux auto), aperçu 3D tournant (STL, OBJ, PLY, OFF, glTF/GLB, FBX binaire), impression via la boîte « Imprimer » quand elle est présente. Miniatures dans l'Explorateur (images, modèles 3D, vidéos/audio) avec cache disque. Garde-fou : HEIC/HEIF, AVIF, JPEG XL, JPEG 2000 et PDF ne sont pas encore décodés, donc jamais annoncés ni envoyés à la visionneuse. `winlux --image-info FICHIER` pour diagnostiquer.
- [x] Moteur multimédia du runtime (`winlux_media.rs`, exports `WinluxMedia*`) : FFmpeg système en processus isolé, conteneurs MP4/MKV/WebM/AVI/MOV/FLV/WMV/MPEG/TS/M2TS/OGV/3GP…, codecs H.264/HEVC/AV1/VP8/VP9/MPEG-2/MPEG-4/VC-1/Theora/ProRes/DNxHD et AAC/MP3/Opus/Vorbis/FLAC/ALAC/PCM/AC-3/E-AC-3/DTS/WMA/APE ; un format n'est annoncé que si son décodeur existe ; VA-API/NVDEC avec repli logiciel puis sans son ; vignettes vidéo et pochettes avec cache disque. Voir `docs/MEDIA.md`.
- [x] Corriger `SetTimer` : la version bouchon de `user32_ext` remplaçait la vraie et `GetMessage` ne livrait jamais `WM_TIMER` (aucune animation ni rafraîchissement périodique des applications autonomes).

### Priorité 3 — Double-clic et intégration Linux

- [x] Installer le runtime et le lanceur dans un chemin stable (`/usr/bin` par défaut, préfixe et `DESTDIR` configurables), avec association MIME par défaut dans `/etc/xdg/mimeapps.list`; validé dans une racine temporaire par `scripts/test-integration.sh`.
- [x] Ajouter une procédure de désinstallation propre du binfmt, du MIME et du fichier `.desktop` (`integration/uninstall.sh`, restaure les associations précédentes).
- [x] Lancement croisé Linux ↔ Windows (#164, `runtime/src/launch.rs`) : un `.exe` lance une appli Linux avec arguments, environnement, dossier courant, code de retour et tubes (`CreatePipe`, `STARTF_USESTDHANDLES`, `PeekNamedPipe`, fin de tube `ERROR_BROKEN_PIPE`, vrai `DuplicateHandle`), recherche du programme comme Windows puis dans le `PATH` Linux, chemins `C:\…` des arguments convertis en chemins Linux, lanceurs `.desktop`, protocoles (`mailto:` → client mail, `steam:`/`magnet:`… → `xdg-open`, `file:///`), `ShellExecuteEx` avec handle de processus. Dans l'autre sens, les chemins Linux donnés à un `.exe` deviennent `C:\…`/`Z:\…` (`WINLUX_KEEP_LINUX_PATHS=1` pour garder les chemins bruts).
- [x] Gérer les chemins contenant des espaces ou des URI `file://`, les fichiers illisibles et les volumes montés `noexec` (le lanceur lit le fichier au lieu de l’exécuter; le dossier du programme devient le répertoire courant).
- [x] Définir le comportement du terminal : `winlux --inspect` lit le sous-système PE, `winlux-open` ouvre un terminal (Konsole, GNOME Terminal, XFCE, xterm ou `WINLUX_TERMINAL`) qui reste ouvert pour les programmes console et lance directement les programmes graphiques.
- [x] Ajouter une détection du fichier `.exe` et des messages d’erreur graphiques (kdialog, zenity, notify-send, xmessage) ou textuels adaptés.

### Priorité 4 — ISO de test

- [x] Construire une base Linux minimale en initramfs pour l’ISO de validation.
- [x] Construire automatiquement l’arborescence de staging.
- [x] Installer Winlux, les dix PE, leur DLL et Xvfb dans cette arborescence.
- [x] Intégrer binfmt, MIME et `.desktop` dans une future image de bureau; ils ne sont pas nécessaires à l’image de validation.
- [x] Ajouter un démarrage BIOS et UEFI avec GRUB.
- [x] Lancer automatiquement les tests séquentiels, parallèles et X11 dans la VM.
- [x] Générer l’ISO avec `xorriso` et produire une somme SHA-256.
- [x] Ajouter un test automatisé dans QEMU avec vérification des marqueurs de résultat.
- [x] Sélectionner l'entrée de diagnostic dans `scripts/test-live-iso-qemu.sh` et vérifier BIOS/UEFI sans modifier l'ISO livrée.
- [x] Documenter la RAM, le nombre de CPU et les options nécessaires à la VM.
- [x] Mode récupération « Winlux Recovery » : réparer GRUB (BIOS, UEFI, Secure Boot), restaurer le point précédent, terminal de secours, fsck (`docs/RECOVERY.md`).
- [x] Vérification d'intégrité du média au démarrage : manifeste `winlux.sha256` par fichier, entrée GRUB dédiée.
- [x] Installation silencieuse : fichier de réponse YAML/JSON, pages préconfigurées sautées, compte local uniquement (`docs/AUTOINSTALL.md`, `scripts/test-autoinstall.py`).
- [x] Installateur : benchmark matériel (note 1,0 à 9,9), profils d'utilisation (Gaming, Bureautique, Développement, Serveur) et migration réseau depuis un autre PC (#101, `scripts/test-installer-oobe.py`).
- [x] Secure Boot : shim signé, GRUB signé, MokManager, clé MOK et signature du noyau, ISO hybride `WINLUX_SECUREBOOT=1` validée sous OVMF (`docs/SECURE-BOOT.md`, `scripts/test-secureboot-qemu.sh`).

### Priorité 5 — Thème Windows 11

- [x] Barre des tâches de la session native : horloge lisible (genmon `winlux-clock`, texte clair fixé, bloc « Winlux panel » de gtk.css resynchronisé à chaque session), panneau volant Notifications + calendrier façon Windows 11 au clic sur l'horloge ou la cloche (mois navigable, jour courant mis en avant, croix par notification, Ne pas déranger), modes d'alimentation Économie / Normal / Turbo détectés (`winlux-power-modes` : power-profiles-daemon, tuned, puis gouverneurs cpufreq/EPP ; section cachée sans choix réel), panneau Son caché sans sortie réelle et mélangeur par application (une barre par flux PipeWire/Pulse qui joue). Test de contraste sur les vrais pixels de la barre.
- [x] Accessibilité : page Paramètres > Accessibilité (aperçu en direct, navigation au clavier), 4 thèmes de contraste (shell, barre des tâches, menu Démarrer et `GetSysColor` des .exe), Loupe plein écran/lentille/ancrée (Win+Plus/Moins/Échap, lissage HALFTONE, inversion), Narrateur `winlux-speak` (espeak-ng ou piper : fenêtre active, texte survolé, frappes, Insérer+flèches), touches rémanentes (Maj ×5) et filtres, taille/couleur du pointeur X11, épaisseur du curseur de texte, animations désactivables, sous-titres et clignotement pour les sons système. Réglages partagés dans `~/.local/share/winlux/accessibility.ini`.
- [x] Planificateur de tâches maison (service `winlux --tasks daemon`, sans systemd ni cron) : déclencheurs horaires, intervalle, démarrage, ouverture de session et événements système (réseau, secteur/batterie, USB), conditions secteur/réseau, historique, application graphique Windows 11 FR/EN.
- [x] Pare-feu maison sur nftables (`winlux --firewall daemon`) : profils domaine/privé/public, règles entrantes/sortantes par port, protocole et application, journal des connexions bloquées, alerte à la première écoute d'une application, application graphique Windows 11 FR/EN.
- [x] Paramètres > Processeur : fréquences par cœur en direct, mode d'alimentation (gouverneur/EPP), placement P-cores/E-cores, températures, bridage thermique, ventilateurs et courbes, NUMA, puis priorité/cœurs/affinité/nœud par programme. Garde-fous : `runtime/src/cpu.rs` détecte le matériel (`/sys`, `/proc/cpuinfo`, CPUID) et une section n'apparaît que si la capacité existe (testé sur faux arbres `/sys` : VM, CPU classique, hybride Intel, NUMA, capteurs absurdes).
- [x] VM : pointeur flèche visible (curseur logiciel Xorg `SWcursor`, flèche dessinée à l’échelle de l’interface).
- [x] VM : la résolution suit la taille de la fenêtre QEMU (nouveau mode préféré appliqué par `xrandr`, bureau relu via `GetSystemMetrics` et réagencé); vérifié en passant de 1920×1080 à 1280×800 dans QEMU.
- [x] Refaire l’interface du bureau façon Windows 11 : fond « fleur », barre des tâches sombre centrée ou à gauche (`WINLUX_TASKBAR_ALIGN=left` ou bouton du menu), menu Démarrer flottant (recherche, épinglés, recommandés, utilisateur, alimentation), zone de notification et icônes vectorielles.
- [x] Mémoriser l’alignement de la barre des tâches et ouvrir de vraies applications depuis la barre et le menu.
- [x] Ajouter un prototype de bureau GDI HiDPI : fond d'écran, raccourcis,
- [x] Reprendre le prototype avec un système visuel professionnel : surfaces
- [x] Charger les vraies icônes PNG des applications depuis leurs fichiers
- [x] Utiliser Rust/GDI avec backend X11 et Xft pour le prototype actuel ; le choix d'une pile de bureau définitive reste ouvert.
- [x] Créer les composants du prototype : barre des tâches, menu Démarrer, fenêtres, notifications et Explorer ; le CSS reste une référence séparée.
- [x] Ajouter les icônes et polices libres compatibles avec la licence du projet.
- [x] Définir les variantes clair/sombre et le réglage d'échelle.
- [x] Intégrer le thème dans l’ISO sans dépendance propriétaire.
- [x] Rendre le menu Démarrer plus compact, animer les accents violets et afficher
- [x] Fenêtre « Bienvenue » au premier login (`welcome.rs`) : visite guidée en 8 étapes (Démarrer, Explorer, Paramètres, Terminal, raccourcis, confidentialité, choix du thème clair/sombre et de l’alignement), rouvrable depuis Démarrer (#98).
- [x] Premier démarrage guidé (#162) : Bienvenue passe à 10 étapes avec « Lancer un programme Windows » (.exe, .msi, lecteurs C:/Z:) et « Vos premiers logiciels » (10 logiciels courants installés en un clic via le Store, sans produit Microsoft, logiciels sans source masqués). File d'attente persistante (`winlux-store starter`) qui continue fenêtre fermée et reprend au login suivant ; la visite reprend à l'étape où on l'a fermée.
- [x] Paramètres > Mises à jour système : mode automatique, manuel ou planifié (jour/heure) pour APT (`winlux-sysupdate`) et Winlux, liste des paquets, journal et notification quand des mises à jour sont disponibles (#98).
- [x] Manettes : `xinput1_4` (état, vibration, capacités, touches) et `dinput8`/DirectInput 7 (clavier, souris, manettes, joysticks, volants) sur evdev (#79).
- [x] Webcam : Media Foundation (MFEnumDeviceSources, source et lecteur synchrone/asynchrone, RGB32/NV12/YUY2/MJPG) sur V4L2, en respectant l'autorisation caméra (#79).
- [x] Micro : waveIn complet (formats PCM/flottant, callbacks fonction/événement/fenêtre, capacités, position) (#79).
- [x] Paramètres > Bluetooth et appareils (BlueZ : activer, rechercher, appairer, connecter, oublier) et indicateur dans la barre des tâches (#79).
- [x] Paramètres > Confidentialité : télémétrie et historique de localisation désactivés par défaut, autorisations micro/caméra/localisation des .exe (micro refusé appliqué par `winmm`) (#98).
- [x] Personnalisation (Paramètres > Personnalisation et assistant de premier démarrage) : galerie de 13 fonds d'écran calculés (catégories Nature/Abstrait/Sombre/Clair, aperçu), palette d'accentuation Windows 11 (48 couleurs ou automatique, option sur Démarrer/barre et barres de titre), barre des tâches en bas/haut/gauche/droite en trois tailles, choix des icônes du bureau, cinq sons de démarrage synthétisés avec écoute. Préférences dans `~/.local/share/winlux/personnalisation.conf` ; `WINLUX_OOBE=1` relance l'assistant, `WINLUX_NO_OOBE=1` le saute.
- [x] Permettre le déplacement des icônes du prototype de bureau (placement
- [x] Store unique Linux + Windows (`store.rs`, `winlux-store`) : apt + AppStream, Flatpak, Snap, AppImage et catalogues Windows interchangeables (`store-sources.d`, SHA-256 obligatoire), vignettes et captures, fiche détaillée, mises à jour unifiées ; voir `docs/STORE.md`. Catalogue Windows par défaut : dépôt communautaire Scoop Extras (applications 64 bits avec empreinte, sans script PowerShell).
- [x] msiexec intégré au runtime (`runtime/src/msi`, sans composant tiers) : fichier composé OLE, tables et pool de chaînes, cabinets MSZIP et LZX, propriétés, conditions, fonctionnalités et composants, fichiers, registre, raccourcis .lnk, « Programmes installés », désinstallation `/x`, installation administrative `/a` ; double-clic sur un .msi ; voir `docs/MSIEXEC.md`. Ajouté ensuite (#179) : AppSearch (registre, .ini, dossiers), fichiers .ini, variables d'environnement (visibles par les .exe), RemoveFile et RemoveRegistry, tout annulé à la désinstallation. Reste : actions personnalisées DLL/EXE, services, interface graphique d'installation.
- [x] Installateurs .exe NSIS 3 et Inno Setup 1.x–7.x interprétés par le runtime (`runtime/src/setup`, LZMA/LZMA2/BCJ maison) : fichiers, composants et tâches, raccourcis, .ini, registre, clé Uninstall, désinstalleur, double-clic ; testés Notepad++, WinMerge, ShareX, Audacity ; voir `docs/SETUP-EXE.md` (#179). Reste : plugins NSIS et programmes lancés par l'installateur.
- [x] Fenêtre « Applications installées » (`programs.rs`, `appwiz.cpl`) : toutes les clés Uninstall (msi, NSIS, Inno, autres), recherche, tri par nom/taille/date, désinstallation en un clic avec le bon outil, traduite FR/EN/ES/DE (#179).
- [x] Gestionnaire de polices (`fonts.rs`, `winlux-fonts`) et polices pour les .exe : `EnumFontFamilies(Ex)`, `EnumFonts`, `AddFontResource(Ex)`, `RemoveFontResource(Ex)`, `WinluxDrawImageA`.

### Priorité 6 — Qualité et livraison

- [x] Ajouter une CI pour `cargo check`, les builds des `.exe` de test et les tests sous Winlux.
- [x] Normaliser le code existant avec `rustfmt`, traiter la dette Clippy puis rendre ces contrôles bloquants dans la CI.
- [x] Ajouter des logs structurés et un niveau de verbosité configurable.
- [x] Documenter la compilation, l’installation, le dépannage et la génération ISO.
- [x] Ajouter un paquet de mise à jour GitHub vérifié par SHA-256, une installation
- [x] Créer une VM graphique persistante QCOW2 issue de la racine Winlux.
- [x] Ajouter une licence GPL-3.0-or-later.
- [x] Fournir BUILD-INFO et sa consultation dans Paramètres → À propos.
- [x] Sécuriser l'installateur et valider le disque installé sans ISO en BIOS/UEFI sous QEMU, avec arrêt automatique.
- [x] Ajouter les barrières de tests Live/disque installé au workflow de release ; son exécution complète sur les runners GitHub reste à valider.
- [x] Ajouter le fichier NOTICES au dépôt et copier les notices audio disponibles dans les ISO.
- [x] Publier ISO, sommes, paquet, manifeste et notes dans les préversions système existantes (notamment preview.4).
- [x] Ajouter un système d'internationalisation (locale.rs) avec tables de chaînes FR/EN/ES/DE (150 entrées).
- [x] Ajouter une page Localisation dans les Paramètres (choix langue, 8 claviers, 12 fuseaux horaires).
- [x] Remplacer les chaînes françaises codées en dur par des appels `tr()` dans le bureau, l'explorateur et le gestionnaire de fenêtres.
- [x] Étendre winlux-control.sh avec les actions lang-*/tz-* et 8 dispositions clavier.
- [x] Ajouter le choix de langue et clavier à l'installateur (install.sh).
- [x] Lire locale.conf au démarrage de session (xsession-winlux, export WINLUX_LANG).
- [x] Presse-papiers Win32 réel partagé entre tous les programmes (fichier de session, `GetClipboardSequenceNumber`, CF_TEXT/CF_UNICODETEXT synthétisés, CF_DIB, CF_HDROP, formats enregistrés stables) et historique Win+V du bureau : texte, images et fichiers, recherche, épingles persistantes, « Tout effacer », limite 10/25/50/100, recoller d'un clic dans la fenêtre active. Ctrl+C/X/V du Bloc-notes et copie de fichiers de l'Explorateur passent par le presse-papiers système.
- [x] Presse-papiers commun Windows ↔ Linux (`winlux --clipboard-bridge`, lancé avec la session) : texte, HTML, images (PNG/BMP, import JPEG/GIF/WebP/TIFF) et fichiers (couper/copier, collage dans Thunar) passent dans les deux sens entre un .exe et une appli Linux ; les copies Linux entrent dans l'historique Win+V (#159).
- [x] Glisser-déposer entre applis Windows et Linux, dans les deux sens et entre deux .exe : dépôt XDND (Thunar, bureau, navigateur) livré aux .exe par `IDropTarget` ou `WM_DROPFILES` (`DragAcceptFiles`, `DragQueryFile`, `DragQueryPoint`), `DoDragDrop` maison vers les fenêtres Linux ; presse-papiers OLE réel (`OleSetClipboard`, `OleGetClipboard`, `IDataObject` maison) utilisé par .NET/WinForms/WPF (#159).
- [x] Outil Capture (Win+Maj+S, Impr. écran, menu Démarrer) : écran figé et assombri, modes rectangle, fenêtre, plein écran et forme libre, copie automatique dans le presse-papiers, éditeur rapide (rognage, stylo, surligneur, flèche, texte, couleurs, tailles, annuler, copier, PNG dans Images\Captures d'écran). Extensions runtime `WinluxCaptureScreen`, `WinluxDrawStroke`, `WinluxDrawText`, `WinluxSavePng`.
- [x] Enregistrement vidéo de l'écran (Win+Maj+R ou bouton caméra de l'outil Capture) : zone ou plein écran, micro et son système (PulseAudio/PipeWire), MP4 (H.264/AAC) ou WebM (VP9/Opus) via le ffmpeg du système, indicateur rouge avec minuteur et bouton stop dans la barre des tâches, fichier dans Vidéos\Enregistrements d'écran. L'ISO embarque ffmpeg s'il est présent sur la machine de construction.


---

*Mis à jour depuis TODO.md. À synchroniser à chaque commit.*
