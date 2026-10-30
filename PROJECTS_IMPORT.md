# Import pour GitHub Projects classique

Copier ces cartes dans le projet "Winlux - Suivi" (colonnes: À faire / En cours / Fait)

## À FAIRE (depuis Suivi.md)
- [ ] Exécuter le bureau de l'ISO en utilisateur non privilégié avec autorisation ciblée des actions système (le script Xsession existe, mais l'init lance encore le bureau en root).
- [ ] Signer les mises à jour et utiliser des racines A/B pour les coupures d'alimentation (voir `docs/AB_UPDATES.md`).
- [ ] Continuer à remplacer les implémentations partielles de `kernel32`, `msvcrt` et `ntdll`; ces modules ne constituent pas des DLL Windows complètes.
- [ ] Implémenter une pile de secours réellement sûre pour les débordements de pile; le gestionnaire de signal est sans allocation/verrou, mais la protection contre le débordement n’est pas validée.
- [ ] Compléter COM/OLE : activation depuis le registre, serveurs hors processus, automatisation et objets OLE applicatifs.
- [ ] Implémenter `StartServiceCtrlDispatcher` et le canal de contrôle/notification des vrais services Windows.
- [ ] Annuler les I/O déjà commencées et tester exhaustivement les buffers/handles invalides.
- [ ] Compléter le cycle de vie des processus : héritage de handles, attributs étendus et suivi/récolte robuste des enfants.
- [ ] Ajouter le partage d’objets nommés et de fichiers entre processus invités.
- [ ] Continuer `ntdll` : exceptions/unwinding, TLS statique des DLL chargées dynamiquement sur chaque thread, sections et primitives bas niveau manquantes.
- [ ] Intégrer un véritable runtime CLR Windows et les couches WPF/WinForms; le pont Linux `dotnet`/Mono ne suffit pas pour `paint.net`.
- [ ] Ajouter les DLL système nécessaires aux logiciels ciblés et mesurer leur compatibilité; la présence de stubs ne garantit pas l’API.
- [ ] Ajouter les autres formats audio compressés et les codecs vidéo ; Media Foundation/DirectShow ne constituent pas encore une chaîne de lecture complète.
- [ ] Implémenter le vrai déchargement/référencement de DLL dans `FreeLibrary` (elles restent mappées jusqu’à la fin du processus).
- [ ] Faire tourner des programmes C++ MinGW (exceptions : `RtlUnwindEx`, `RtlVirtualUnwind`, `RtlLookupFunctionEntry`) et des exécutables MSVC/clang-cl réels.
- [ ] Valider l’installation réelle sur l’hôte (`sudo integration/install.sh`) ; les tests en racine temporaire ne constituent pas cette preuve.
- [ ] Tester l’enregistrement binfmt sur une machine Linux réelle ; script de diagnostic et configuration présents, résultat réel non établi par cet audit.
- [ ] Tester le double-clic depuis GNOME, KDE et XFCE ; la présence de `test-binfmt-desktops.sh` ne prouve pas ces trois sessions.
- [ ] Compléter l'accessibilité : contraste, taille du pointeur, réduction des animations et navigation accessible complète.
- [ ] Revalider le rendu du bureau courant en 1080p, 1440p et 4K, avec captures et interactions ; les tests X11 généraux ne prouvent pas cette matrice complète.
- [ ] Démarrer avec un bureau vide ; rendre les icônes système facultatives et
- [ ] Ajouter la création explicite de raccourcis d'applications, leur
- [ ] Vérifier et embarquer toutes les notices et les obligations de fourniture de sources pour une distribution complète ; NOTICES seul n'est pas une preuve d'empaquetage exhaustif.
- [ ] Valider une exécution complète du workflow de release modifié sur GitHub ; ses étapes de construction et tests ne constituent pas une publication déjà effectuée.
- [ ] Signer les mises à jour (Ed25519) et implémenter le schéma A/B réel (voir `docs/AB_UPDATES.md`).
- [ ] Committer une première version stable après validation du pipeline complet.


## DÉJÀ FAIT (récents)
- [x] Générer l’ISO avec `xorriso` et produire une somme SHA-256.
- [x] Ajouter un test automatisé dans QEMU avec vérification des marqueurs de résultat.
- [x] Sélectionner l'entrée de diagnostic dans `scripts/test-live-iso-qemu.sh` et vérifier BIOS/UEFI sans modifier l'ISO livrée.
- [x] Documenter la RAM, le nombre de CPU et les options nécessaires à la VM.
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
- [x] Permettre le déplacement des icônes du prototype de bureau (placement
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
