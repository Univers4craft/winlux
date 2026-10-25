# Thème et prototype de bureau Winlux

La palette CSS de ce dossier est une référence visuelle, pas le moteur du
bureau. Le prototype actuel est écrit en Rust/GDI dans `testexe/src/shell/`
et rendu par X11/Xft. Il est embarqué dans les ISO de test ; GTK/Qt ne sont
pas des dépendances de ce shell.

## Réalisé dans le prototype

- [x] Fond d'écran image BMP fourni par le constructeur depuis un PNG, avec repli vectoriel animé.
- [x] Barre des tâches pleine largeur, menu Démarrer ouvrable/fermable et fenêtres Explorer/Paramètres.
- [x] États réduit, maximisé, restauré, empilement et focus des fenêtres internes.
- [x] Thèmes clair/sombre et alignement de la barre enregistrés par `winlux-control` sur racine persistante.
- [x] Texte antialiasé Xft/Fontconfig et échelle DPI configurable ; repli sur les polices X11 core.
- [x] Paramètres Linux/Windows, mises à jour et À propos avec métadonnées de construction.
- [x] Intégration dans une ISO bootable testée en BIOS/UEFI.

## Reste à faire

- [ ] Rendre les trois icônes système facultatives et démarrer avec un bureau vide.
- [ ] Créer/renommer/supprimer des raccourcis applicatifs et sauvegarder leurs positions.
- [ ] Ajouter le placement libre configurable ; le déplacement actuel est recalé sur une grille.
- [ ] Alimenter un catalogue d'applications installées, une vraie recherche Démarrer et des épingles modifiables.
- [ ] Relier un centre d'applications aux gestionnaires Linux et aux installateurs/désinstallateurs Windows.
- [ ] Compléter l'accessibilité, la navigation clavier et la matrice de tests visuels 1080p/1440p/4K.
- [ ] Gérer les fenêtres Linux externes et le presse-papiers inter-applications comme un environnement de bureau complet.
- [ ] Exécuter le bureau ISO en utilisateur non administrateur avec autorisations ciblées.

La vue Applications actuelle n'est pas un gestionnaire APT/Flatpak/EXE/MSI
fonctionnel. Les trois icônes de bureau restent prédéfinies et leurs positions
temporaires. Le script Xsession permet un lancement par un display manager,
mais ne suffit pas à sécuriser le démarrage root de l'ISO.

Audit du 25 octobre 2026 : [preuves et réserves](../docs/AUDIT-MARKDOWN.md).
Voir aussi [INTERFACE.md](../INTERFACE.md) pour les objectifs complets.
